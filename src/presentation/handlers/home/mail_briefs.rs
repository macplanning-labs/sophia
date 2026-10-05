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
    Extension(_user): Extension<AuthUser>,
) -> impl IntoResponse {
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
