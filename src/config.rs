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
            secret_key: std::env::var("SECRET_KEY")
                .unwrap_or_else(|_| "change-me-in-production".to_string()),
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
