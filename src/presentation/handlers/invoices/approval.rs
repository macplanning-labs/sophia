/// invoices/approval.rs — 請求書承認/差戻し（Admin限定）

use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
};
use sqlx::PgPool;

use crate::presentation::middleware::role::AuthUser;
use crate::presentation::api_response::AppError;

/// POST /api/invoices/{id}/approve — 承認（Admin限定、PENDING_APPROVAL→APPROVED）
pub async fn api_approve(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    use crate::infrastructure::repositories::billing_repo::approve_invoice;

    match approve_invoice(&pool, id, auth_user.user.id).await {
        Ok(true) => Ok(axum::Json(serde_json::json!({ "success": true })).into_response()),
        Ok(false) => Ok((axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
            "success": false, "error": "承認待ちの請求書のみ承認できます（既に承認済み、または送信済みの可能性があります）"
        }))).into_response()),
        Err(e) => Err(AppError::from(e)),
    }
}

/// POST /api/invoices/{id}/reject — 差戻し（Admin限定、APPROVED→PENDING_APPROVAL）
pub async fn api_reject(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::infrastructure::repositories::billing_repo::reject_invoice;

    match reject_invoice(&pool, id).await {
        Ok(true) => Ok(axum::Json(serde_json::json!({ "success": true })).into_response()),
        Ok(false) => Ok((axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
            "success": false, "error": "承認済みの請求書のみ差し戻せます"
        }))).into_response()),
        Err(e) => Err(AppError::from(e)),
    }
}
