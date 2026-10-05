/// invoices/crud.rs — 請求書のヘッダ更新（JSON）・削除
///
/// 請求書の作成は、決済画面の一括発行（settlement_dashboard::create_invoices_by_client）で行う。
/// 旧SSRの作成・編集（未ルーティングで、旧スキーマの列名を使っていた）は、削除した。

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Json,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;
use serde::Deserialize;

use crate::infrastructure::repositories::billing_repo;
use crate::presentation::api_response::AppError;

#[derive(Debug, Deserialize)]
pub struct ApiUpdateInvoiceHeaderForm {
    pub issue_date: String,
    pub due_date: Option<String>,
    pub subject: String,
}

/// PUT /api/v1/invoices/{id} — ヘッダ情報の更新（JSON。明細は対象外・件名/発行日/支払期日のみ）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Json(form): Json<ApiUpdateInvoiceHeaderForm>,
) -> Result<impl IntoResponse, AppError> {
    let issue_date = match chrono::NaiveDate::parse_from_str(&form.issue_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "success": false, "error": "発行日の形式が不正です"
            }))).into_response());
        }
    };
    let due_date = form.due_date.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    billing_repo::update_invoice_header(&pool, id, issue_date, due_date, &form.subject).await?;
    tracing::info!("請求書ヘッダ更新: id={}", id);

    Ok(Json(serde_json::json!({ "success": true })).into_response())
}

/// POST /invoices/{id}/delete — 削除
pub async fn delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // 支払通知(パートナー未受諾のみ削除可)と同じ方針: クライアントが受領確認する前なら、
    // 承認済み・送付済みでも削除可とする（送付＝メールが届いただけで、相手方が実際に
    // 見て確定させたとは限らないため。受領確認済みは正式な文書として扱い削除不可）。
    let invoice = billing_repo::find_invoice(&pool, id).await.log_err().ok().flatten();
    match invoice {
        Some(ref inv) if inv.client_accepted_at.is_none() => {}
        Some(_) => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false, "error": "クライアント受領確認前の請求書のみ削除できます"
            }))).into_response());
        }
        None => {}
    }

    billing_repo::delete_invoice(&pool, id).await?;
    tracing::info!("請求書削除: id={}", id);

    Ok(Redirect::to("/invoices").into_response())
}
