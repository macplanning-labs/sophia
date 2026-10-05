/// config.rs — アプリケーション設定 + 共有ステート
///
/// 環境変数から設定値を読み込む。
/// .env ファイルまたは Docker の環境変数で設定する。
/// AppState は axum Router の共有ステートとして使用する。

use sqlx::PgPool;

// ── AppState（axum 共有ステート）──

/// アプリケーション全体で共有するステート
///
/// axum の `State<AppState>` で注入する。
/// 既存ハンドラの `State<PgPool>` は FromRef トレイトで互換性を維持する。
///
/// WebAuthn の Relying Party 設定はここでは持たない。RP ID/Origin はリクエストの
/// Host ヘッダーから都度解決する（`webauthn_service::create_webauthn_from_headers`）。
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub secret_key: String,
    pub portal_attempt_store: std::sync::Arc<dyn auth_core::domain::attempt_lock::AttemptStore>,
    pub mfa_attempt_store: std::sync::Arc<dyn auth_core::domain::attempt_lock::AttemptStore>,
}

/// 既存の State<PgPool> ハンドラとの互換性を維持する
impl axum::extract::FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> PgPool {
        state.pool.clone()
    }
}

// ── 設定 ──

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub port: u16,
    pub base_url: String,
    pub secret_key: String,

    // SMTP設定（.env フォールバック。CompanyInfo DB優先）
    pub email_host: String,
    pub email_port: u16,
    pub email_use_tls: bool,
    pub email_host_user: String,
    pub email_host_password: String,
    pub default_from_email: String,

    // Webhook
    pub webhook_secret: String,

    // IMAP設定（受信メール取得用 — EDI_MP email_receiver.py 移植）
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_user: String,
    pub imap_password: String,
}

/// 公開されている(=誰でも知っている)SECRET_KEY の値。以前、見本(.env.example / .env.rust.example)と
/// コードの既定値に書いていたもの。これが設定されていたら、未設定と同じく起動を止める(DEMO-000097)。
const KNOWN_PUBLIC_SECRET_KEYS: &[&str] = &[
    // secret-default-ok: 拒否するための一覧。この値を使うことは無い
    "change-me",
    "change-me-in-production",
    "change-me-to-a-long-random-string",
];

/// SECRET_KEY はログインの証明(JWT)の署名と、二段階認証の秘密の暗号化に使う。
/// 既定値を持たせると、公開された時点で誰でも知っている鍵になる(GHSA-hp9g-vf5r-43wq と同じ種類)。
/// 未設定・空・公開済みの値なら、起動を止める。
fn require_secret_key(value: Option<String>) -> anyhow::Result<String> {
    let v = value.unwrap_or_default();
    if v.trim().is_empty() {
        anyhow::bail!(
            "SECRET_KEY が未設定です。ログインの証明の署名と二段階認証の暗号化に使うため必須です。\
             .env.<ENV_NAME> に、openssl rand -hex 32 などで作った値を設定してください"
        );
    }
    if KNOWN_PUBLIC_SECRET_KEYS.contains(&v.as_str()) {
        anyhow::bail!(
            "SECRET_KEY に、公開されている見本の値が設定されています。誰でも知っている値なので使えません。\
             openssl rand -hex 32 などで作った値に替えてください(替えると、全員がいったんログアウトします)"
        );
    }
    Ok(v)
}

impl AppConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        let base_url = std::env::var("BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8111".to_string());

        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?,
            port: std::env::var("PORT")
                .unwrap_or_else(|_| "8111".to_string())
                .parse()?,
            base_url,
            secret_key: require_secret_key(std::env::var("SECRET_KEY").ok())?,
            email_host: std::env::var("EMAIL_HOST")
                .unwrap_or_else(|_| "smtp.gmail.com".to_string()),
            email_port: std::env::var("EMAIL_PORT")
                .unwrap_or_else(|_| "587".to_string())
                .parse()?,
            email_use_tls: std::env::var("EMAIL_USE_TLS")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            email_host_user: std::env::var("EMAIL_HOST_USER")
                .unwrap_or_default(),
            email_host_password: std::env::var("EMAIL_HOST_PASSWORD")
                .unwrap_or_default(),
            default_from_email: std::env::var("DEFAULT_FROM_EMAIL")
                .unwrap_or_default(),
            webhook_secret: std::env::var("WEBHOOK_SECRET")
                .unwrap_or_default(),

            // IMAP: 専用環境変数 → SMTP設定フォールバック
            imap_host: std::env::var("IMAP_HOST")
                .unwrap_or_else(|_| "imap.gmail.com".to_string()),
            imap_port: std::env::var("IMAP_PORT")
                .unwrap_or_else(|_| "993".to_string())
                .parse()?,
            imap_user: std::env::var("IMAP_USER")
                .or_else(|_| std::env::var("EMAIL_HOST_USER"))
                .unwrap_or_default(),
            imap_password: std::env::var("IMAP_PASSWORD")
                .or_else(|_| std::env::var("EMAIL_HOST_PASSWORD"))
                .unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod secret_key_tests {
    use super::*;

    #[test]
    fn missing_or_empty_secret_key_stops_startup() {
        assert!(require_secret_key(None).is_err());
        assert!(require_secret_key(Some(String::new())).is_err());
        assert!(require_secret_key(Some("   ".to_string())).is_err());
    }

    #[test]
    fn publicly_known_secret_keys_are_rejected() {
        for k in KNOWN_PUBLIC_SECRET_KEYS {
            assert!(
                require_secret_key(Some((*k).to_string())).is_err(),
                "公開済みの値が通ってしまう"
            );
        }
    }

    #[test]
    fn a_proper_secret_key_is_accepted() {
        let k = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"; // secret-default-ok: テスト用の値
        assert_eq!(require_secret_key(Some(k.to_string())).unwrap(), k);
    }
}
