//! Google サービスアカウント JWT 認証 → OAuth2 アクセストークン取得
//!
//! Sophia の `src/infrastructure/google_auth.rs` から移植。
//! ドメイン全体委任（`impersonate`）とスコープを引数で汎用パラメータ化しているため、
//! Drive / Sheets / Gmail など任意の Google API スコープで共用できる。

use crate::error::{GoogleAuthError, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Google Service Account JSON キー
#[derive(Debug, Deserialize)]
pub struct ServiceAccountKey {
    pub client_email: String,
    pub private_key: String,
    pub token_uri: String,
}

/// JWT クレーム
#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    sub: Option<String>,
}

/// トークンレスポンス
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// サービスアカウントキーからアクセストークンを取得する
///
/// `impersonate`: ドメイン全体委任のなりすまし先メールアドレス（不要な場合は `None`）
/// `scope`: 要求するスコープ（例: `"https://www.googleapis.com/auth/drive"`）
pub async fn get_access_token(
    key: &ServiceAccountKey,
    impersonate: Option<&str>,
    scope: &str,
) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = JwtClaims {
        iss: key.client_email.clone(),
        scope: scope.to_string(),
        aud: key.token_uri.clone(),
        exp: now + 3600,
        iat: now,
        sub: impersonate.map(|s| s.to_string()),
    };

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .map_err(GoogleAuthError::Jwt)?;
    let jwt = jsonwebtoken::encode(&header, &claims, &encoding_key).map_err(GoogleAuthError::Jwt)?;

    let client = reqwest::Client::new();
    let resp = client
        .post(&key.token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ])
        .send()
        .await?;

    let token: TokenResponse = resp.json().await?;
    Ok(token.access_token)
}

/// 環境変数で指定されたサービスアカウント認証情報ファイルから、指定スコープのアクセストークンを取得する。
///
/// `credentials_file_env`: 認証情報ファイルパスを指定する環境変数名（例: `"GOOGLE_DRIVE_CREDENTIALS_FILE"`）
/// `impersonate`: ドメイン全体委任のなりすまし先メールアドレス（オプション）
/// `scope`: 要求するスコープ
///
/// 戻り値:
/// - `Ok(Some(token))`: 認証情報が設定・読み込め、トークン取得成功
/// - `Ok(None)`: 認証情報ファイル未設定または不存在（呼び出し元は機能をスキップ）
/// - `Err(e)`: 認証情報ファイルの読み込み・パース・トークン取得の失敗
pub async fn get_access_token_from_file(
    credentials_file_env: &str,
    impersonate: Option<&str>,
    scope: &str,
) -> Result<Option<String>> {
    let credentials_file = std::env::var(credentials_file_env).unwrap_or_default();
    if credentials_file.is_empty() || !std::path::Path::new(&credentials_file).exists() {
        info!(
            "[Google API] {} が未設定のためスキップ",
            credentials_file_env
        );
        return Ok(None);
    }

    let key_json = std::fs::read_to_string(&credentials_file)?;
    let key: ServiceAccountKey = serde_json::from_str(&key_json)?;

    let token = get_access_token(&key, impersonate, scope).await?;
    Ok(Some(token))
}
