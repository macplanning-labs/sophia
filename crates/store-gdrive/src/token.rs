//! アクセストークンの取り方(差し替えられるようにする: テストでは固定のトークンを使う)

use async_trait::async_trait;
use integration_core::StoreError;

/// アクセストークンを返すもの
#[async_trait]
pub trait TokenSource: Send + Sync {
    async fn token(&self, scope: &str) -> Result<String, StoreError>;
}

/// サービスアカウントの鍵(JSON)でトークンを取る。
/// 鍵は秘密なので Debug を実装しない。
pub struct ServiceAccountTokenSource {
    key: google_auth::ServiceAccountKey,
    impersonate: Option<String>,
}

impl ServiceAccountTokenSource {
    /// 鍵の JSON(文字列)から作る。`impersonate` はドメイン全体の委任のなりすまし先(不要なら None)。
    pub fn from_json(key_json: &str, impersonate: Option<String>) -> Result<Self, StoreError> {
        let key: google_auth::ServiceAccountKey = serde_json::from_str(key_json)
            // serde のエラー文は鍵の中身を含まない(位置と型だけ)
            .map_err(|e| StoreError::Auth(format!("サービスアカウントの鍵を読めません: {e}")))?;
        Ok(Self { key, impersonate: impersonate.filter(|s| !s.trim().is_empty()) })
    }

    /// 鍵のファイルから作る
    pub fn from_file(path: &std::path::Path, impersonate: Option<String>) -> Result<Self, StoreError> {
        let s = std::fs::read_to_string(path)
            .map_err(|e| StoreError::Auth(format!("サービスアカウントの鍵のファイルを読めません({}): {e}", path.display())))?;
        Self::from_json(&s, impersonate)
    }

    /// 鍵のメールアドレス(共有ドライブのメンバー追加の案内に使う。秘密ではない)
    pub fn client_email(&self) -> &str {
        &self.key.client_email
    }
}

#[async_trait]
impl TokenSource for ServiceAccountTokenSource {
    async fn token(&self, scope: &str) -> Result<String, StoreError> {
        google_auth::get_access_token(&self.key, self.impersonate.as_deref(), scope)
            .await
            .map_err(|e| StoreError::Auth(format!("アクセストークンを取れません: {e}")))
    }
}

/// 固定のトークン(テスト用)
pub struct StaticToken(pub String);

#[async_trait]
impl TokenSource for StaticToken {
    async fn token(&self, _scope: &str) -> Result<String, StoreError> {
        Ok(self.0.clone())
    }
}
