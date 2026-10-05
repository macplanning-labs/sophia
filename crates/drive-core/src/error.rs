//! Google Drive連携エラー型定義

/// Google Drive連携エラー
#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("サービスアカウントキーの読み込みに失敗: {0}")]
    CredentialsRead(#[from] std::io::Error),

    #[error("サービスアカウントキーの解析に失敗: {0}")]
    CredentialsParse(String),

    #[error("Drive認証エラー: {0}")]
    AuthFailed(String),

    #[error("Drive APIエラー: {0}")]
    ApiError(String),

    #[error("HTTP通信エラー: {0}")]
    HttpError(#[from] reqwest::Error),
}

/// `Result<T>` — DriveError を既定エラー型とする結果型
pub type Result<T> = std::result::Result<T, DriveError>;
