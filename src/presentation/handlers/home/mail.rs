/// home/mail.rs — メールチェック・確認

use axum::{
    extract::{Extension, State, Path, Json},
    response::IntoResponse,
    http::StatusCode,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use crate::presentation::middleware::role::AuthUser;

use super::StatusResponse;

use crate::infrastructure::mail_pipeline::{circuit_breaker, imap_util::load_imap_config};

pub use crate::infrastructure::repositories::mail_repo::MailRow;

impl MailRow {
    pub fn classification_label(&self) -> &str {
        match self.classification.as_str() {
            "ORDER" => "受注書",
            "INVOICE" => "請求書",
            "REPORT" => "報告書",
            "CONTRACT" => "契約書",
            _ => "その他",
        }
    }

    pub fn classification_badge_class(&self) -> &str {
        match self.classification.as_str() {
            "ORDER" => "bg-primary-subtle text-primary",
            "INVOICE" => "bg-info-subtle text-info",
            "REPORT" => "bg-success-subtle text-success",
            "CONTRACT" => "bg-warning-subtle text-warning",
            _ => "bg-secondary-subtle text-secondary",
        }
    }

    pub fn received_date_str(&self) -> String {
        crate::domain::services::jst::format(self.received_at, "%m/%d %H:%M")
    }
}

pub(super) async fn build_mail_logs(pool: &PgPool) -> Vec<MailRow> {
    crate::infrastructure::repositories::mail_repo::list_unreflected_mails(pool)
        .await
        .unwrap_or_else(|e| { tracing::error!("home: list_unreflected_mails failed: {:?}", e); vec![] })
}

pub(super) async fn count_confirmed_mails(pool: &PgPool) -> i64 {
    crate::infrastructure::repositories::mail_repo::count_confirmed_mails_recent(pool)
        .await
        .log_err().unwrap_or(0)
}

pub(super) async fn count_needs_review_mails(pool: &PgPool) -> i64 {
    match crate::infrastructure::repositories::received_email_repo::count_needs_manual_review(pool).await {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("[メール取込/Dashboard] needs_review 件数取得失敗: {}", e);
            0
        }
    }
}

pub(super) async fn count_fetch_failed_mails(pool: &PgPool) -> i64 {
    match crate::infrastructure::repositories::received_email_repo::count_fetch_failed(pool).await {
        Ok(count) => count,
        Err(e) => {
            tracing::warn!("[メール取込/Dashboard] fetch_failed 件数取得失敗: {}", e);
            0
        }
    }
}

pub(super) async fn mail_sync_last_processed_at(pool: &PgPool) -> Option<chrono::DateTime<chrono::Utc>> {
    crate::infrastructure::repositories::mail_repo::latest_sync_checkpoint(pool)
        .await
        .unwrap_or_else(|e| {
            tracing::error!(
                "[メール取込/Dashboard] 処理=同期チェックポイント読取 結果=失敗 影響=サイレント検知が効かない可能性 | {}",
                e
            );
            None
        })
}

/// POST /api/mail/fetch — 手動メール取得（IMAP）
///
/// ダッシュボードの「メールチェック」ボタンから呼ばれる。
/// スケジューラと同じ mail_pipeline::run_pipeline（Phase1〜4）を実行する。
pub async fn manual_mail_fetch(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
) -> axum::response::Response {
    // 担当スタンプを持たない操作(一覧・選択肢・新規作成)。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::MailReceivedEdit) {
        return crate::presentation::http_util::forbidden();
    }
    axum::response::IntoResponse::into_response(manual_mail_fetch_inner(State(pool)).await)
}

async fn manual_mail_fetch_inner(
    State(pool): State<PgPool>,
) -> (StatusCode, Json<StatusResponse>) {
    let result = crate::infrastructure::mail_pipeline::run_pipeline(&pool).await;
    let has_errors = result.has_errors();
    let msg = result.to_string();
    (StatusCode::OK, Json(StatusResponse {
        ok: !has_errors,
        error: Some(msg),
    }))
}

#[derive(serde::Serialize)]
pub struct ImapLockStatus {
    pub locked: bool,
    pub reason: Option<String>,
}

/// GET /api/mail/imap-lock-status — IMAPサーキットブレイカーのロック状態
pub async fn imap_lock_status(State(pool): State<PgPool>) -> Json<ImapLockStatus> {
    let mailbox = load_imap_config(&pool).await.ok().map(|c| c.mailbox_id().to_string());

    let reason = match mailbox {
        Some(m) => circuit_breaker::locked_reason(&pool, &m).await,
        None => None,
    };

    Json(ImapLockStatus { locked: reason.is_some(), reason })
}

/// POST /api/mail/imap-unlock — IMAPサーキットブレイカーのロックを管理者が手動解除する
///
/// 連続認証失敗による自動ロックは、原因（認証情報の誤り等）を確認したうえで
/// 管理者がここから明示的に解除するまで有効なまま（自動時間経過での解除はしない）。
pub async fn imap_unlock(
    State(pool): State<PgPool>,
) -> (StatusCode, Json<StatusResponse>) {
    let Some(mailbox) = load_imap_config(&pool).await.ok().map(|c| c.mailbox_id().to_string()) else {
        return (StatusCode::BAD_REQUEST, Json(StatusResponse {
            ok: false,
            error: Some("IMAP設定が読み込めません".into()),
        }));
    };

    match circuit_breaker::unlock(&pool, &mailbox).await {
        Ok(_) => (StatusCode::OK, Json(StatusResponse { ok: true, error: None })),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
            ok: false,
            error: Some(e.to_string()),
        })),
    }
}

/// POST /api/mail/imap-test — IMAP接続確認（実際のメール取得は行わない。ロック状態には影響しない）
///
/// `imap_util::connect_and_examine` はブロッキングI/Oのため spawn_blocking の中で実行する
/// （Tokioワーカースレッド占有による504タイムアウト再発防止。connect_and_examineのdocコメント参照）。
pub async fn imap_test(State(pool): State<PgPool>) -> (StatusCode, Json<StatusResponse>) {
    let config = match load_imap_config(&pool).await {
        Ok(c) => c,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(StatusResponse { ok: false, error: Some(e) }));
        }
    };

    let result = tokio::task::spawn_blocking(move || {
        crate::infrastructure::mail_pipeline::imap_util::connect_and_examine(&config)
            .map(|mut session| { let _ = session.logout(); })
    })
    .await;

    match result {
        Ok(Ok(())) => (StatusCode::OK, Json(StatusResponse { ok: true, error: None })),
        Ok(Err(e)) => (StatusCode::OK, Json(StatusResponse { ok: false, error: Some(e) })),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
            ok: false,
            error: Some(format!("内部エラー: {e}")),
        })),
    }
}

/// POST /api/mail/{id}/confirm — メールを確認済みにする
pub async fn confirm_mail(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    // 受信メールは担当スタンプを持たない。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::MailReceivedEdit) {
        return crate::presentation::http_util::forbidden();
    }
    axum::response::IntoResponse::into_response(confirm_mail_inner(&pool, id).await)
}

async fn confirm_mail_inner(pool: &PgPool, id: i64) -> impl IntoResponse {
    let result = crate::infrastructure::repositories::mail_repo::set_mail_reflected(pool, id, true).await;

    match result {
        Ok(rows) if rows > 0 => {
            (StatusCode::OK, Json(StatusResponse { ok: true, error: None }))
        }
        _ => {
            (StatusCode::NOT_FOUND, Json(StatusResponse {
                ok: false,
                error: Some("メールが見つかりません".into()),
            }))
        }
    }
}

/// POST /api/mail/{id}/unconfirm — 確認済みを取り消す
pub async fn unconfirm_mail(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> axum::response::Response {
    // 受信メールは担当スタンプを持たない。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::MailReceivedEdit) {
        return crate::presentation::http_util::forbidden();
    }
    axum::response::IntoResponse::into_response(unconfirm_mail_inner(&pool, id).await)
}

async fn unconfirm_mail_inner(pool: &PgPool, id: i64) -> impl IntoResponse {
    let result = crate::infrastructure::repositories::mail_repo::set_mail_reflected(pool, id, false).await;

    match result {
        Ok(rows) if rows > 0 => {
            (StatusCode::OK, Json(StatusResponse { ok: true, error: None }))
        }
        _ => {
            (StatusCode::NOT_FOUND, Json(StatusResponse {
                ok: false,
                error: Some("メールが見つかりません".into()),
            }))
        }
    }
}

