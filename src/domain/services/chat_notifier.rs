/// domain/services/chat_notifier.rs — Google Chat Incoming Webhook への汎用テキスト投稿
///
/// メール(SMTP)経由のアラートはメールアカウント自体の認証情報が壊れていると
/// 一緒に送信不能になる。Google Chat Webhookはメールアカウントの
/// 認証情報と無関係な別チャネルのため、致命的アラートはこちらも併用する。
use reqwest;
use crate::infrastructure::mail_pipeline::ops_message;

/// `GOOGLE_CHAT_WEBHOOK_URL` 未設定・空のときは何もしない(既存の任意機能としての扱いを踏襲)。
/// URLはログに出さない([SEC-13.2])。
pub async fn post(text: &str) {
    let url = match std::env::var("GOOGLE_CHAT_WEBHOOK_URL") {
        Ok(u) if !u.trim().is_empty() => u,
        _ => {
            tracing::debug!("[ChatNotifier] GOOGLE_CHAT_WEBHOOK_URL未設定 — Chat投稿スキップ");
            return;
        }
    };

    let client = reqwest::Client::new();
    match client
        .post(&url)
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            tracing::info!("[ChatNotifier] Google Chat投稿完了");
        }
        Ok(resp) => {
            tracing::error!(
                "{}",
                ops_message::fail(
                    "Chat通知",
                    "Webhook",
                    "Google Chat投稿",
                    "チャネル経由の障害通知が届いていない可能性",
                    format!("HTTP {}", resp.status()),
                )
            );
        }
        Err(e) => {
            tracing::error!(
                "{}",
                ops_message::fail(
                    "Chat通知",
                    "Webhook",
                    "Google Chat投稿",
                    "チャネル経由の障害通知が届いていない可能性",
                    format!("{}", e),
                )
            );
        }
    }
}
