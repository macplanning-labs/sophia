//! presentation/handlers/billing.rs — 請求書のプレビューと確定(UI刷新 2-4 / DEMO-000136)
//!
//! - `GET  /api/v1/billing/preview?month=YYYY-MM[&client_id=N]` — 承認済みの勤務表から、請求書の中身を計算して返す(保存しない)
//! - `POST /api/v1/billing/confirm` — 取引先×月の排他ロックの中で再計算し、請求書を作る。強制確定は理由必須で履歴を残す
//!
//! どちらも管理者専用(`admin_routes`)。旧画面の `settlement/issue-invoices` は残している(別の経路)。

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::domain::services::billing_confirm::{
    confirm_invoices, load_previews, parse_month, ConfirmError, ConfirmOutcome, ConfirmRequest,
};
use crate::presentation::api_response::ApiError;
use crate::presentation::middleware::role::AuthUser;

#[derive(Debug, Deserialize)]
pub struct PreviewQuery {
    /// "YYYY-MM"
    pub month: Option<String>,
    pub client_id: Option<i64>,
}

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ApiError { success: false, error: message.into() })).into_response()
}

/// 請求書のプレビュー(保存しない)
pub async fn api_preview(State(pool): State<PgPool>, Query(q): Query<PreviewQuery>) -> Response {
    let Some(month) = q.month.as_deref().and_then(parse_month) else {
        return error_response(StatusCode::BAD_REQUEST, "month は YYYY-MM の形で指定してください");
    };

    match load_previews(&pool, month, q.client_id).await {
        Ok((_, previews)) => Json(serde_json::json!({
            "month": month.format("%Y-%m").to_string(),
            "invoices": previews,
        }))
        .into_response(),
        Err(e) => {
            tracing::error!("billing preview failed: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "請求書のプレビューを計算できませんでした")
        }
    }
}

/// 確定の結果から、本文(JSON)を作る
fn outcome_body(success: bool, error: Option<&str>, outcome: &ConfirmOutcome) -> serde_json::Value {
    let mut body = serde_json::json!({
        "success": success,
        "created": outcome.created,
        "skipped": outcome.skipped,
    });
    if let Some(e) = error {
        body["error"] = serde_json::Value::String(e.to_string());
    }
    body
}

/// 請求書の確定
pub async fn api_confirm(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Json(req): Json<ConfirmRequest>,
) -> Response {
    match confirm_invoices(&pool, auth_user.user.id, &req).await {
        Ok(outcome) if outcome.created.is_empty() && !outcome.skipped.is_empty() => {
            // 何も作れなかった。揃っていない場合は「あと誰か」を skipped に載せて 409 で返す
            let blocked = outcome.skipped.iter().any(|s| s.reason == "NOT_CONFIRMABLE");
            let message = if blocked {
                "勤務表が揃っていないため確定できません。理由を入力すると強制確定できます"
            } else {
                "確定できる請求書がありませんでした"
            };
            (StatusCode::CONFLICT, Json(outcome_body(false, Some(message), &outcome))).into_response()
        }
        Ok(outcome) => Json(outcome_body(true, None, &outcome)).into_response(),
        Err(ConfirmError::BadRequest(m)) => error_response(StatusCode::BAD_REQUEST, m),
        Err(ConfirmError::Internal(m)) => {
            tracing::error!("billing confirm failed: {m}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "請求書の確定に失敗しました。画面を更新して、もう一度お試しください",
            )
        }
    }
}
