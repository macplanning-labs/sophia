/// presentation/handlers/company_info.rs — 自社情報(s_company_info)設定画面 JSON API
///
/// - GET /api/company-info — 取得
/// - PUT /api/company-info — 更新

use axum::{extract::{Extension, State}, response::IntoResponse, Json};
use sqlx::PgPool;

use crate::infrastructure::repositories::company_info_repo::{self, CompanyInfoUpdate};
use crate::infrastructure::mail_pipeline::{circuit_breaker, imap_util::ImapConfig};
use crate::presentation::middleware::role::AuthUser;

/// GET /api/company-info
pub async fn api_get(State(pool): State<PgPool>) -> impl IntoResponse {
    match company_info_repo::get_company_info(&pool).await {
        Ok(Some(info)) => Json(serde_json::json!({ "success": true, "company_info": info })).into_response(),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "自社情報が登録されていません"
        }))).into_response(),
        Err(e) => {
            tracing::error!("api_get company_info: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "success": false, "error": "取得に失敗しました"
            }))).into_response()
        }
    }
}

/// PUT /api/company-info
pub async fn api_update(
    State(pool): State<PgPool>,
    Json(form): Json<CompanyInfoUpdate>,
) -> impl IntoResponse {
    let existing = match company_info_repo::get_company_info(&pool).await {
        Ok(Some(info)) => info,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "自社情報が登録されていません"
        }))).into_response(),
        Err(e) => {
            tracing::error!("api_update company_info (fetch): {:?}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "success": false, "error": "更新に失敗しました"
            }))).into_response();
        }
    };

    match company_info_repo::update_company_info(&pool, existing.id, &form).await {
        Ok(_) => {
            // SMTPパスワードが更新された場合、IMAPロックを自動解除
            if form.email_host_password.is_some() {
                if let Ok(config) = ImapConfig::load(&pool).await {
                    let mailbox = config.mailbox_id().to_string();
                    if let Err(e) = circuit_breaker::unlock(&pool, &mailbox).await {
                        tracing::warn!("[CompanyInfo] IMAPロック自動解除失敗: {}", e);
                    } else {
                        tracing::info!("[CompanyInfo] パスワード更新に伴いIMAPロックを自動解除しました");
                    }
                }
            }
            Json(serde_json::json!({ "success": true })).into_response()
        }
        Err(e) => {
            tracing::error!("api_update company_info: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "success": false, "error": "更新に失敗しました"
            }))).into_response()
        }
    }
}

/// POST /api/company-info/test-email — 現在保存済みのSMTP設定でテストメールを送信する
///
/// 宛先は常にログイン中の管理者自身のメールアドレス固定
/// （任意宛先を受け付けると、この管理者専用APIが第三者へのメール送信手段に転用されうるため）。
pub async fn api_test_email(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let to = auth_user.user.email.clone();
    if to.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "ログイン中のアカウントにメールアドレスが設定されていません"
        }))).into_response();
    }

    let email_svc = crate::domain::services::email_service::EmailService::new(pool);
    let subject = "【Sophia】テストメール（SMTP設定確認）";
    let body = "この画面のSMTP設定でテストメールを送信しました。\nこのメールが届いていれば、SMTP送信設定は正常です。\n";

    match email_svc.send(&to, None, subject, body).await {
        Ok(_) => Json(serde_json::json!({ "success": true, "to": to })).into_response(),
        Err(e) => {
            tracing::warn!("[CompanyInfo] テストメール送信失敗: {:?}", e);
            (axum::http::StatusCode::OK, Json(serde_json::json!({
                "success": false, "error": e.to_string()
            }))).into_response()
        }
    }
}
