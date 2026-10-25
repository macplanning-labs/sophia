/// home/mail_briefs.rs — 取引先別メールスレッド要約カード API

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Serialize;
use sqlx::PgPool;

use crate::presentation::middleware::role::AuthUser;
use crate::infrastructure::repositories::mail_brief_repo;

/// GET /api/v1/mail-briefs レスポンス
#[derive(Debug, Serialize)]
pub struct MailBriefsResponse {
    pub briefs: Vec<mail_brief_repo::MailThreadBrief>,
}

/// 取引先別メールスレッド要約を取得
///
/// スタッフ認証必須。Ollama 呼び出しは非同期で（GET はブロックしない）。
pub async fn get_mail_briefs(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<crate::domain::authz::Authz>>,
) -> impl IntoResponse {
    // 受信メールの要約は担当スタンプを持たない。権限を持つ人だけが見られる。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::MailReceivedView) {
        return crate::presentation::http_util::forbidden();
    }
    match mail_brief_repo::fetch_all_mail_thread_briefs(&pool).await {
        Ok(briefs) => (StatusCode::OK, Json(MailBriefsResponse { briefs })).into_response(),
        Err(e) => {
            tracing::error!("Failed to fetch mail briefs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "メール要約の取得に失敗しました"})),
            )
                .into_response()
        }
    }
}
