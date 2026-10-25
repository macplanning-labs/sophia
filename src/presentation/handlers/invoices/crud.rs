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

use crate::domain::services::sent_document_lock;
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
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
    Json(form): Json<ApiUpdateInvoiceHeaderForm>,
) -> Result<impl IntoResponse, AppError> {
    // 入口の権限表に頼らず、ハンドラ自身で権限と担当範囲を判定する(DEMO-000254)
    let stamp = match crate::infrastructure::repositories::stamp_repo::of_invoice(&pool, id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("担当スタンプの取得に失敗: {e:#}");
            return Ok(crate::presentation::http_util::forbidden());
        }
    };
    if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::BillingInvoiceEdit, &stamp) {
        return Ok(crate::presentation::http_util::forbidden());
    }
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

    // 相手に送ったあとは書き換えない(DEMO-000223)
    match billing_repo::find_invoice(&pool, id).await.log_err().ok().flatten() {
        Some(inv) if sent_document_lock::invoice_is_locked(inv.sent_at, inv.client_accepted_at) => {
            return Ok((axum::http::StatusCode::CONFLICT, Json(serde_json::json!({
                "success": false, "error": sent_document_lock::edit_denied_message("請求書")
            }))).into_response());
        }
        Some(_) => {}
        None => {
            return Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
                "success": false, "error": "請求書が見つかりません"
            }))).into_response());
        }
    }

    billing_repo::update_invoice_header(&pool, id, issue_date, due_date, &form.subject).await?;
    tracing::info!("請求書ヘッダ更新: id={}", id);

    Ok(Json(serde_json::json!({ "success": true })).into_response())
}

/// POST /invoices/{id}/delete — 削除
pub async fn delete(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // 入口の権限表に頼らず、ハンドラ自身で担当範囲を判定する(AUTHZ_MODE が shadow でも拒否する。DEMO-000254)
    let stamp = match crate::infrastructure::repositories::stamp_repo::of_invoice(&pool, id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("担当スタンプの取得に失敗: {e:#}");
            return Ok(crate::presentation::http_util::forbidden());
        }
    };
    if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::BillingInvoiceEdit, &stamp) {
        return Ok(crate::presentation::http_util::forbidden());
    }
    // 相手に送る前だけ削除できる。送ったあと・承諾したあとは、削除しない(何がダメだったかが分からなくなるため。
    // 内容を変えるときは、新しい版を作って、もう一度送る)。DEMO-000223
    let invoice = billing_repo::find_invoice(&pool, id).await.log_err().ok().flatten();
    match invoice {
        Some(ref inv) if !sent_document_lock::invoice_is_locked(inv.sent_at, inv.client_accepted_at) => {}
        Some(_) => {
            return Ok((axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
                "success": false, "error": sent_document_lock::delete_denied_message("請求書")
            }))).into_response());
        }
        None => {}
    }

    billing_repo::delete_invoice(&pool, id).await?;
    tracing::info!("請求書削除: id={}", id);

    Ok(Redirect::to("/invoices").into_response())
}
