//! presentation/handlers/mail_settings.rs — 「メール」画面の設定と送信の記録(DEMO-000196。管理者専用)
//!
//! - GET/PUT /api/v1/mail/account          — メールアカウント(送信と受信の取り込みで共通。パスワードの実値は返さない)
//! - GET/PUT /api/v1/mail/receive-settings — 受信の設定(取引先として扱わないドメイン。GET は受信サーバ・アカウント・取り込みの状態も返す)
//! - GET     /api/v1/mail/sent             — 送信メールの一覧(新しい順)
//! - GET     /api/v1/mail/sent/{id}        — 送信メール1件(本文つき)
//!
//! どれも s_company_info / h_sent_email を読み書きする。メールの設定は自社情報の画面から外した。

use axum::{extract::{Path, State}, http::StatusCode, Json};
use sqlx::PgPool;

use crate::infrastructure::repositories::company_info_repo::{self, MailAccountUpdate};
use crate::infrastructure::mail_pipeline::{circuit_breaker, imap_util};
use crate::infrastructure::repositories::{mail_repo, sent_email_repo};
use crate::presentation::handlers::company_info::unlock_imap_after_password_change;

type ApiResult = Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>;

fn err(status: StatusCode, msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "success": false, "error": msg })))
}

fn internal(ctx: &str, e: anyhow::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("{ctx}: {e:?}");
    err(StatusCode::INTERNAL_SERVER_ERROR, "処理に失敗しました")
}

const NO_COMPANY: &str = "自社情報が登録されていません";

/// 除外ドメインの入力の上限(文字数)。1行1ドメインで数百件を想定
const EXCLUDED_DOMAINS_MAX_CHARS: usize = 10_000;

/// GET /api/v1/mail/account
pub async fn get_account(State(pool): State<PgPool>) -> ApiResult {
    match company_info_repo::get_mail_account(&pool).await {
        Ok(Some(a)) => Ok(Json(serde_json::json!({ "success": true, "account": a }))),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, NO_COMPANY)),
        Err(e) => Err(internal("mail get_account", e)),
    }
}

/// PUT /api/v1/mail/account
pub async fn put_account(State(pool): State<PgPool>, Json(form): Json<MailAccountUpdate>) -> ApiResult {
    if let Some(p) = form.email_port {
        if !(1..=65535).contains(&p) {
            return Err(err(StatusCode::BAD_REQUEST, "ポートは 1〜65535 の数で入力してください"));
        }
    }
    match company_info_repo::update_mail_account(&pool, &form).await {
        Ok(0) => Err(err(StatusCode::NOT_FOUND, NO_COMPANY)),
        Ok(_) => {
            if form.changes_password() {
                unlock_imap_after_password_change(&pool).await;
            }
            Ok(Json(serde_json::json!({ "success": true })))
        }
        Err(e) => Err(internal("mail put_account", e)),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ReceiveSettingsUpdate {
    pub excluded_mail_domains: String,
}

/// GET /api/v1/mail/receive-settings
///
/// 受信の ⚙ に出すもの: 除外ドメイン(編集できる)と、受信サーバ・アカウント・取り込みの状態(見るだけ)。
/// アカウントは送信と共通(メール > 送信 の ⚙ で変える)。
pub async fn get_receive_settings(State(pool): State<PgPool>) -> ApiResult {
    let raw = company_info_repo::get_excluded_mail_domains_raw(&pool)
        .await
        .map_err(|e| internal("mail get_receive_settings", e))?;
    let account = company_info_repo::get_mail_account(&pool)
        .await
        .map_err(|e| internal("mail get_receive_settings (account)", e))?;
    let (host, port) = imap_util::imap_server();
    let last_processed_at = mail_repo::latest_sync_checkpoint(&pool)
        .await
        .map_err(|e| internal("mail get_receive_settings (checkpoint)", e))?;
    // ロックはアカウントごと。アカウントが未設定なら、ロックも無い
    let lock_reason = match imap_util::load_imap_config(&pool).await {
        Ok(c) => circuit_breaker::locked_reason(&pool, c.mailbox_id()).await,
        Err(_) => None,
    };
    let user = account.as_ref().map(|a| a.email_host_user.clone()).unwrap_or_default();
    let configured = account.as_ref().is_some_and(|a| !a.email_host_user.is_empty() && a.has_smtp_password);
    Ok(Json(serde_json::json!({
        "success": true,
        "excluded_mail_domains": raw,
        "imap": { "host": host, "port": port, "user": user, "configured": configured },
        "last_processed_at": last_processed_at,
        "locked": lock_reason.is_some(),
        "lock_reason": lock_reason,
    })))
}

/// PUT /api/v1/mail/receive-settings
pub async fn put_receive_settings(State(pool): State<PgPool>, Json(form): Json<ReceiveSettingsUpdate>) -> ApiResult {
    if form.excluded_mail_domains.chars().count() > EXCLUDED_DOMAINS_MAX_CHARS {
        return Err(err(StatusCode::BAD_REQUEST, "除外するドメインが長すぎます"));
    }
    match company_info_repo::set_excluded_mail_domains(&pool, &form.excluded_mail_domains).await {
        Ok(0) => Err(err(StatusCode::NOT_FOUND, NO_COMPANY)),
        Ok(_) => Ok(Json(serde_json::json!({ "success": true }))),
        Err(e) => Err(internal("mail put_receive_settings", e)),
    }
}

/// GET /api/v1/mail/sent
pub async fn list_sent(State(pool): State<PgPool>) -> ApiResult {
    let rows = sent_email_repo::list_recent(&pool).await.map_err(|e| internal("mail list_sent", e))?;
    Ok(Json(serde_json::json!({ "success": true, "emails": rows, "limit": sent_email_repo::LIST_LIMIT })))
}

/// GET /api/v1/mail/sent/{id}
pub async fn get_sent(State(pool): State<PgPool>, Path(id): Path<i64>) -> ApiResult {
    match sent_email_repo::find_by_id(&pool, id).await {
        Ok(Some(m)) => Ok(Json(serde_json::json!({ "success": true, "email": m }))),
        Ok(None) => Err(err(StatusCode::NOT_FOUND, "送信メールが見つかりません")),
        Err(e) => Err(internal("mail get_sent", e)),
    }
}
