//! google-auth エラー型定義

use thiserror::Error;

/// Google認証エラー
#[derive(Debug, Error)]
pub enum GoogleAuthError {
    #[error("認証情報ファイルの読み込みに失敗: {0}")]
    Io(#[from] std::io::Error),

    #[error("認証情報の解析に失敗: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JWT生成に失敗: {0}")]
    Jwt(jsonwebtoken::errors::Error),

    #[error("トークンリクエストに失敗: {0}")]
    Http(#[from] reqwest::Error),
}

/// `Result<T>` — GoogleAuthError を既定エラー型とする結果型
pub type Result<T> = std::result::Result<T, GoogleAuthError>;
