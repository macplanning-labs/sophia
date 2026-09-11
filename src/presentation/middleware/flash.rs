/// presentation/middleware/flash.rs — Flash メッセージ
///
/// Cookie ベースの一時メッセージ。
/// POST 後のリダイレクトで成功/エラーメッセージを表示。

use serde::{Deserialize, Serialize};

/// Flash メッセージ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlashMessage {
    pub level: String,   // "success" | "error" | "warning" | "info"
    pub message: String,
}

impl FlashMessage {
    pub fn success(message: impl Into<String>) -> Self {
        Self { level: "success".to_string(), message: message.into() }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { level: "danger".to_string(), message: message.into() }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self { level: "warning".to_string(), message: message.into() }
    }

    /// Cookie値にエンコード
    pub fn to_cookie_value(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Cookie値からデコード
    pub fn from_cookie_value(value: &str) -> Option<Self> {
        serde_json::from_str(value).ok()
    }
}
