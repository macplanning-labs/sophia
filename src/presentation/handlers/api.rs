/// presentation/handlers/api.rs — API エンドポイント
///
/// POST /api/webhook/timesheet — GASからの稼働報告メール受信通知
/// HMAC-SHA256 署名検証（X-Webhook-Signature ヘッダ）で認証し、
/// 受信メール情報をDBに記録する。

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
    body::Bytes,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::info;

type HmacSha256 = Hmac<Sha256>;

/// Webhook受信（HMAC-SHA256 署名検証）
pub async fn index(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let secret = std::env::var("WEBHOOK_SECRET").unwrap_or_default();

    // HMAC-SHA256 署名検証
    if !secret.is_empty() {
        let signature = headers
            .get("X-Webhook-Signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // フォールバック: 旧方式（単純シークレット比較）
        let old_secret = headers
            .get("X-Webhook-Secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !signature.is_empty() {
            // HMAC-SHA256 検証
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
                .expect("HMAC can take key of any size");
            mac.update(&body);
            let expected = hex::encode(mac.finalize().into_bytes());

            if signature != expected {
                return (StatusCode::UNAUTHORIZED, Json(WebhookResponse {
                    status: "error".to_string(),
                    message: "Invalid HMAC signature".to_string(),
                }));
            }
        } else if old_secret != secret {
            return (StatusCode::UNAUTHORIZED, Json(WebhookResponse {
                status: "error".to_string(),
                message: "Unauthorized".to_string(),
            }));
        }
    }

    // JSON パース
    let payload: TimesheetWebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, Json(WebhookResponse {
                status: "error".to_string(),
                message: format!("Invalid JSON: {}", e),
            }));
        }
    };

    info!(
        "[Webhook] 稼働報告メール受信: from={}, subject={}",
        payload.from_email, payload.subject
    );

    // 受信メールをDBに記録
    let result = crate::infrastructure::repositories::received_email_repo::insert_received_email(
        &pool,
        &payload.message_id,
        &payload.from_email,
        &payload.from_name,
        &payload.subject,
        &payload.body_text,
        &payload.attachment_filename,
    ).await;

    match result {
        Ok(_) => {
            info!("[Webhook] 受信メール記録成功: message_id={}", payload.message_id);
            (StatusCode::OK, Json(WebhookResponse {
                status: "ok".to_string(),
                message: "Received".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("[Webhook] DB記録失敗: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(WebhookResponse {
                status: "error".to_string(),
                message: "DB error".to_string(),
            }))
        }
    }
}

#[derive(Deserialize)]
pub struct TimesheetWebhookPayload {
    pub message_id: String,
    pub from_email: String,
    #[serde(default)]
    pub from_name: String,
    pub subject: String,
    #[serde(default)]
    pub body_text: String,
    #[serde(default)]
    pub attachment_filename: String,
}

#[derive(Serialize)]
pub struct WebhookResponse {
    pub status: String,
    pub message: String,
}
