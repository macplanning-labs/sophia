//! notify-gchat — 知らせる部品: Google Chat(Incoming Webhook)
//!
//! - 必要な設定: `webhook_url`(必須・秘密)
//! - 点検: URL の形だけを確かめる。**実際の送信はしない**(相手のスペースに書き込むため)。
//!   届くかどうかは、人が押すテスト送信([`GChatNotifier::send_test`])と、実際の送信の結果で確かめる。
//! - Webhook の URL は鍵を含むので、ログ・エラー・Debug に出さない。

use std::time::Duration;

use async_trait::async_trait;
use integration_core::{CheckReport, Integration, Level, Notice, Notifier, NotifyError, SettingSpec};

/// Google Chat の Incoming Webhook の URL の頭
const WEBHOOK_PREFIX: &str = "https://chat.googleapis.com/v1/spaces/";

const TIMEOUT: Duration = Duration::from_secs(15);

pub struct GChatNotifier {
    webhook_url: String,
    client: reqwest::Client,
}

impl std::fmt::Debug for GChatNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GChatNotifier").field("webhook_url", &"<秘密>").finish()
    }
}

impl GChatNotifier {
    /// 必要な設定
    pub const SETTINGS: &'static [SettingSpec] = &[SettingSpec {
        key: "webhook_url",
        label_ja: "Webhook の URL",
        secret: true,
        required: true,
    }];

    /// 空の URL は受け付けない(未設定は、部品を作る前にアプリ側で「未設定」として扱う)
    pub fn new(webhook_url: impl Into<String>) -> Result<Self, NotifyError> {
        let webhook_url = webhook_url.into().trim().to_string();
        if webhook_url.is_empty() {
            return Err(NotifyError::Config("Webhook の URL が空です".into()));
        }
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| NotifyError::Config(format!("HTTP クライアントを作れません: {e}")))?;
        Ok(Self { webhook_url, client })
    }

    /// 人が押すテスト送信(画面の「テスト送信」ボタン用)。相手のスペースに1件書き込む。
    pub async fn send_test(&self, sender: &str) -> Result<(), NotifyError> {
        let n = Notice::text(Level::Info, format!("【テスト送信】{sender} からの通知の確認です。届いていれば設定は正しいです。"));
        self.notify(&n).await
    }
}

/// 送る文。`Notice::incident` で作ったもの(タイトルあり)は、頭の「絵文字【システム名】レベル」をそのまま使う。
/// タイトルが無い「本文だけ」の通知には、レベルの絵文字を頭に付ける。
fn render(notice: &Notice) -> String {
    if notice.title.is_empty() {
        format!("{} {}", notice.level.emoji(), notice.to_plain_text())
    } else {
        notice.to_plain_text()
    }
}

#[async_trait]
impl Integration for GChatNotifier {
    fn provider(&self) -> &'static str {
        "gchat"
    }

    async fn check(&self) -> CheckReport {
        let r = CheckReport::new();
        if self.webhook_url.starts_with(WEBHOOK_PREFIX)
            && self.webhook_url.contains("key=")
            && self.webhook_url.contains("token=")
        {
            r.pass("Webhook の URL の形")
        } else {
            r.fail(
                "Webhook の URL の形",
                "Google Chat の Incoming Webhook の URL(https://chat.googleapis.com/v1/spaces/…?key=…&token=…)ではありません",
            )
        }
    }
}

#[async_trait]
impl Notifier for GChatNotifier {
    async fn notify(&self, notice: &Notice) -> Result<(), NotifyError> {
        let resp = self
            .client
            .post(&self.webhook_url)
            .json(&serde_json::json!({ "text": render(notice) }))
            .send()
            .await
            // reqwest のエラーは URL を含むことがあるので、URL を外して返す
            .map_err(|e| NotifyError::Transport(e.without_url().to_string()))?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else {
            Err(NotifyError::Remote(format!("HTTP {status}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn empty_url_is_config_error() {
        assert!(matches!(GChatNotifier::new("  "), Err(NotifyError::Config(_))));
    }

    #[tokio::test]
    async fn check_accepts_real_shape_only() {
        let ok = GChatNotifier::new("https://chat.googleapis.com/v1/spaces/AAA/messages?key=k&token=t").unwrap();
        assert!(ok.check().await.ok());
        let ng = GChatNotifier::new("https://hooks.slack.com/services/x").unwrap();
        assert!(!ng.check().await.ok());
    }

    #[test]
    fn debug_hides_url() {
        let n = GChatNotifier::new("https://chat.googleapis.com/v1/spaces/AAA/messages?key=SECRET&token=SECRET").unwrap();
        assert!(!format!("{n:?}").contains("SECRET"));
    }

    #[tokio::test]
    async fn posts_incident_in_the_standard_shape() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(serde_json::json!({
                "text": "🔥【S】High\n概要：a\n影響範囲：b\n発生時刻：2026-10-12 09:00（日本時間）\n次のアクション：c"
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let n = GChatNotifier::new(format!("{}/v1/spaces/x/messages?key=k&token=t", server.uri())).unwrap();
        n.notify(&Notice::incident("S", Level::High, "a", "b", "2026-10-12 09:00", "c")).await.unwrap();
    }

    #[tokio::test]
    async fn bare_text_gets_the_level_emoji() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(body_json(serde_json::json!({ "text": "⚠️ 本文" })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let n = GChatNotifier::new(format!("{}/v1/spaces/x/messages?key=k&token=t", server.uri())).unwrap();
        n.notify(&Notice::text(Level::Warning, "本文")).await.unwrap();
    }

    #[tokio::test]
    async fn http_error_is_remote_and_hides_url() {
        let server = MockServer::start().await;
        Mock::given(method("POST")).respond_with(ResponseTemplate::new(403)).mount(&server).await;
        let url = format!("{}/v1/spaces/x/messages?key=SECRET&token=SECRET", server.uri());
        let n = GChatNotifier::new(url).unwrap();
        let err = n.notify(&Notice::text(Level::Info, "x")).await.unwrap_err();
        assert!(matches!(err, NotifyError::Remote(_)));
        assert!(!err.to_string().contains("SECRET"));
    }

    #[tokio::test]
    async fn transport_error_hides_url() {
        // 何も待ち受けていないポートへ送る
        let n = GChatNotifier::new("http://127.0.0.1:9/v1/spaces/x/messages?key=SECRET&token=SECRET").unwrap();
        let err = n.notify(&Notice::text(Level::Info, "x")).await.unwrap_err();
        assert!(matches!(err, NotifyError::Transport(_)));
        assert!(!err.to_string().contains("SECRET"));
    }
}
