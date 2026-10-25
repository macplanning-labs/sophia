/// presentation/handlers/company_info.rs — 自社情報(s_company_info)設定画面 JSON API
///
/// - GET /api/company-info — 取得
/// - PUT /api/company-info — 更新

use axum::{extract::{Extension, State}, response::IntoResponse, Json};
use sqlx::PgPool;

use crate::domain::value_objects::account_number::AccountNumber;
use crate::infrastructure::repositories::company_info_repo::{self, CompanyInfoUpdate};
use crate::infrastructure::mail_pipeline::{circuit_breaker, imap_util::load_imap_config};
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
    // 利益の目安の検証（0〜100の整数）
    if let Some(Err(e)) = form.target_margin_direct.map(company_info_repo::validate_margin_target) {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": format!("利益の目安(直受け): {}", e)
        }))).into_response();
    }
    if let Some(Err(e)) = form.target_margin_subcontract.map(company_info_repo::validate_margin_target) {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": format!("利益の目安(下請け): {}", e)
        }))).into_response();
    }

    // 口座番号の形式が違えば、保存せず 400 で返す(拒否を DB 層の 500 にしない。DEMO-000233 S2-G2-2)
    if let Some(msg) = account_number_input_error(&form.account_number) {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": msg
        }))).into_response();
    }

    let existing = match company_info_repo::get_company_info_basic(&pool).await {
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
            if form.email_host_password.as_deref().is_some_and(|p| !p.is_empty()) {
                unlock_imap_after_password_change(&pool).await;
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

/// 口座番号の入力を保存の前に確かめる。形式が違えば利用者向けの文言を返す(値は返さない)。
/// 検証の規則は `AccountNumber::parse_input` に集約している(S2-G2-4)。ここは拒否を 400 にするだけ。
pub(crate) fn account_number_input_error(value: &str) -> Option<&'static str> {
    match AccountNumber::parse_input(value) {
        Ok(_) => None,
        Err(e) => Some(e.message()),
    }
}

/// SMTPパスワード(受信の取り込みにも使う)を変えたあと、連続失敗で掛かった IMAP のロックを解除する
pub(crate) async fn unlock_imap_after_password_change(pool: &PgPool) {
    if let Ok(config) = load_imap_config(pool).await {
        let mailbox = config.mailbox_id().to_string();
        if let Err(e) = circuit_breaker::unlock(pool, &mailbox).await {
            tracing::warn!("[CompanyInfo] IMAPロック自動解除失敗: {}", e);
        } else {
            tracing::info!("[CompanyInfo] パスワード更新に伴いIMAPロックを自動解除しました");
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

#[cfg(test)]
mod tests {
    use super::account_number_input_error;

    #[test]
    fn encrypted_looking_account_number_is_refused_with_400_message() {
        // enc1: で始まる値は平文として拒否する(以前は DB 層の Err が 500 になっていた)
        assert_eq!(account_number_input_error("enc1:k1:AAAA"), Some("口座番号の形式が正しくありません"));
        assert!(!account_number_input_error("enc1:k1:AAAA").unwrap_or("").contains("enc1:"));
    }

    #[test]
    fn ordinary_account_number_is_accepted() {
        assert_eq!(account_number_input_error("0012345"), None);
        assert_eq!(account_number_input_error(""), None, "空は未登録");
    }
}
