//! error.rs — auth-core 共通エラー型
//!
//! 利用側アプリはこの型を自アプリのエラー型に変換して
//! HTTPレスポンスにマッピングする想定（例: `{"detail": ...}` のような形に
//! アプリ側で表現を統一する）。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("トークンの有効期限が切れています")]
    TokenExpired,

    #[error("トークンが不正です: {0}")]
    InvalidToken(String),

    #[error("対応していないパスワードハッシュ形式です")]
    UnsupportedHashFormat,

    #[error("パスワードハッシュ処理に失敗しました: {0}")]
    HashError(String),

    #[error("アカウントがロックされています（{retry_after_seconds}秒後に再試行可能）")]
    AccountLocked { retry_after_seconds: u64 },

    #[error("リクエストボディが大きすぎます（上限 {limit_bytes} バイト）")]
    PayloadTooLarge { limit_bytes: usize },

    #[error("TOTP処理に失敗しました: {0}")]
    Totp(String),

    #[error("WebAuthn処理に失敗しました: {0}")]
    WebAuthn(String),

    #[error("設定が不正です: {0}")]
    Config(String),

    #[error("認証情報が見つかりません")]
    NotFound,

    #[error("内部エラー: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;

/// `AuthUser<U>` extractorの`Rejection`など、auth-core内でHTTPレスポンスへ直接
/// 変換したい箇所向けの既定マッピング。アプリ独自のエラー表現を使いたい場合は
/// 呼び出し側でこの型に変換せず、`AuthError`の値そのものをアプリのエラー型に
/// マッピングしてよい。
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let status = match &self {
            AuthError::TokenExpired | AuthError::InvalidToken(_) | AuthError::NotFound => {
                StatusCode::UNAUTHORIZED
            }
            AuthError::AccountLocked { .. } => StatusCode::TOO_MANY_REQUESTS,
            AuthError::PayloadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            AuthError::UnsupportedHashFormat
            | AuthError::HashError(_)
            | AuthError::Totp(_)
            | AuthError::WebAuthn(_)
            | AuthError::Config(_)
            | AuthError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = match &self {
            AuthError::AccountLocked {
                retry_after_seconds,
            } => {
                json!({ "error": "account_locked", "retry_after_seconds": retry_after_seconds })
            }
            AuthError::PayloadTooLarge { limit_bytes } => {
                json!({ "error": "payload_too_large", "limit_bytes": limit_bytes })
            }
            _ => json!({ "detail": self.to_string() }),
        };
        (status, Json(body)).into_response()
    }
}
