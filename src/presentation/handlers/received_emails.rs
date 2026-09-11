/// presentation/handlers/received_emails.rs — 受信メール管理
///
/// Django版 timesheet/presentation/views/emails.py から移植。
/// - GET  /received-emails          — 一覧（ステータスフィルタ）
/// - POST /received-emails/{id}/import — 稼働報告取込

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use sqlx::PgPool;

use crate::infrastructure::repositories::received_email_repo;
use crate::presentation::api_response::AppError;

// ── テンプレート ──


#[derive(Debug, serde::Deserialize)]
pub struct FilterParams {
    pub status: Option<String>,
    pub needs_review: Option<bool>,
    #[serde(default)]
    pub known_domain_only: bool,
}

// ── ハンドラ ──

/// POST /received-emails/{id}/import — 稼働報告取込
pub async fn import(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    // ステータスを IMPORTED に更新
    if let Err(e) = received_email_repo::mark_imported(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to("/received-emails")
}

// ── SPA用 JSON API ──

/// GET /api/received-emails
pub async fn api_index(
    State(pool): State<PgPool>,
    Query(params): Query<FilterParams>,
) -> impl IntoResponse {
    let filter_status = params.status.unwrap_or_default();
    let emails = received_email_repo::list_received_emails(&pool, &filter_status, params.needs_review, params.known_domain_only)
        .await
        .unwrap_or_default();
    axum::Json(serde_json::json!({ "emails": emails }))
}

/// POST /api/received-emails/{id}/import
pub async fn api_import(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = received_email_repo::mark_imported(&pool, id).await?;
    Ok(axum::Json(serde_json::json!({ "success": true, "rows_affected": rows })))
}

/// POST /api/received-emails/{id}/resolve — 「要確認」フラグを手動で解除する
pub async fn api_resolve(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match received_email_repo::mark_manually_resolved(&pool, id).await {
        Ok(rows) if rows > 0 => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Ok(_) => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "success": false, "error": "対象のメールが見つかりません"
        }))).into_response(),
        Err(e) => {
            tracing::error!("api_resolve received_email: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({
                "success": false, "error": "処理に失敗しました"
            }))).into_response()
        }
    }
}

/// DELETE /api/received-emails/{id} — 不要なメールをDBから削除する
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match received_email_repo::delete_received_email(&pool, id).await {
        Ok(rows) if rows > 0 => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Ok(_) => (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "success": false, "error": "対象のメールが見つかりません"
        }))).into_response(),
        Err(e) => {
            tracing::error!("api_delete received_email: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({
                "success": false, "error": "削除に失敗しました"
            }))).into_response()
        }
    }
}
