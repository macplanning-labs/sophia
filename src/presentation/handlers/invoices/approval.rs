/// invoices/approval.rs — 請求書承認/差戻し（Admin限定）

use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
};
use sqlx::PgPool;

use crate::presentation::middleware::role::AuthUser;
use crate::presentation::api_response::AppError;

/// 訂正版・取り消しの、理由の入力
#[derive(Debug, serde::Deserialize, Default)]
pub struct ReasonBody {
    pub reason: Option<String>,
}

fn revision_error_response(code: u16, msg: String) -> axum::response::Response {
    let status = axum::http::StatusCode::from_u16(code).unwrap_or(axum::http::StatusCode::CONFLICT);
    (status, axum::Json(serde_json::json!({ "success": false, "error": msg }))).into_response()
}

/// 取り消し済み・差し替え済みの請求書は、承認・差し戻し・送信などをさせない(DEMO-000224)。操作してよければ None
pub async fn current_guard(pool: &PgPool, id: i64) -> Option<axum::response::Response> {
    use crate::domain::services::document_revision::{not_current_message, RevisionState};
    use crate::infrastructure::repositories::billing_repo;
    let inv = billing_repo::find_invoice(pool, id).await.ok().flatten()?;
    let superseded = billing_repo::find_superseding_invoice(pool, id).await.ok().flatten().is_some();
    let state = RevisionState::of_invoice(inv.sent_at, inv.client_accepted_at, inv.cancelled_at, superseded);
    if state.can_operate() {
        None
    } else {
        Some(revision_error_response(409, not_current_message("請求書", &state)))
    }
}

/// POST /api/invoices/{id}/revise — 送った請求書の訂正版を作る(Admin限定。DEMO-000224)
pub async fn api_revise(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<ReasonBody>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::document_revision::{outcome_error, validate_reason, RevisionOutcome};
    use crate::infrastructure::repositories::billing_repo;
    let reason = match validate_reason(body.reason.as_deref().unwrap_or("")) {
        Ok(r) => r,
        Err(m) => return Ok(revision_error_response(400, m.to_string())),
    };
    let outcome = billing_repo::revise_invoice(&pool, id, &reason).await?;
    if let Some((code, msg)) = outcome_error("請求書", &outcome) {
        return Ok(revision_error_response(code, msg));
    }
    let RevisionOutcome::Done(new_id) = outcome else { return Ok(revision_error_response(500, "訂正版を作れませんでした".into())) };
    tracing::info!("[訂正版] 請求書 {} の訂正版 {} を作りました(理由: {})", id, new_id, reason);
    Ok(axum::Json(serde_json::json!({ "success": true, "new_invoice_id": new_id })).into_response())
}

/// POST /api/invoices/{id}/cancel — 送った請求書を取り消す(Admin限定。DEMO-000224)
pub async fn api_cancel(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Extension(auth_user): Extension<AuthUser>,
    axum::Json(body): axum::Json<ReasonBody>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::document_revision::{outcome_error, validate_reason};
    use crate::infrastructure::repositories::billing_repo;
    let reason = match validate_reason(body.reason.as_deref().unwrap_or("")) {
        Ok(r) => r,
        Err(m) => return Ok(revision_error_response(400, m.to_string())),
    };
    let outcome = billing_repo::cancel_invoice(&pool, id, &reason, auth_user.user.id).await?;
    if let Some((code, msg)) = outcome_error("請求書", &outcome) {
        return Ok(revision_error_response(code, msg));
    }
    tracing::info!("[取り消し] 請求書 {} を取り消しました(理由: {})", id, reason);
    Ok(axum::Json(serde_json::json!({ "success": true })).into_response())
}

/// POST /api/invoices/{id}/approve — 承認（Admin限定、PENDING_APPROVAL/REJECTED→APPROVED）
pub async fn api_approve(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    use crate::infrastructure::repositories::billing_repo::approve_invoice;

    if let Some(r) = current_guard(&pool, id).await {
        return Ok(r);
    }
    match approve_invoice(&pool, id, auth_user.user.id).await {
        Ok(true) => Ok(axum::Json(serde_json::json!({ "success": true })).into_response()),
        Ok(false) => Ok((axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
            "success": false, "error": "承認待ち・差戻しの請求書のみ承認できます（既に承認済み、または送信済みの可能性があります）"
        }))).into_response()),
        Err(e) => Err(AppError::from(e)),
    }
}

/// POST /api/invoices/{id}/reject — 差戻し（Admin限定、PENDING_APPROVAL/APPROVED→REJECTED）
pub async fn api_reject(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::infrastructure::repositories::billing_repo::reject_invoice;

    if let Some(r) = current_guard(&pool, id).await {
        return Ok(r);
    }
    match reject_invoice(&pool, id).await {
        Ok(true) => Ok(axum::Json(serde_json::json!({ "success": true })).into_response()),
        Ok(false) => Ok((axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
            "success": false, "error": "承認待ちまたは承認済みの請求書のみ差し戻せます"
        }))).into_response()),
        Err(e) => Err(AppError::from(e)),
    }
}
