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
    /// ポータル(マジックリンク)の試行。`reserve`(記録してから数える)で判定する
    pub portal_attempt_store: std::sync::Arc<crate::infrastructure::repositories::attempt_lock_store::PgAttemptStore>,
    /// 二要素認証(TOTP)の試行(IP + ユーザー単位)。`reserve` で判定する
    pub mfa_attempt_store: std::sync::Arc<crate::infrastructure::repositories::attempt_lock_store::PgAttemptStore>,
    /// パスキーの失敗(ログインの完了・パスキーによる二要素認証の完了)。IP 単位・`reserve` で判定する
    pub passkey_attempt_store: std::sync::Arc<crate::infrastructure::repositories::attempt_lock_store::PgAttemptStore>,
    /// パスキーログインの開始の回数(IP 単位。成功では消さず、時間窓で明ける。上限は緩め)
    pub passkey_begin_attempt_store: std::sync::Arc<crate::infrastructure::repositories::attempt_lock_store::PgAttemptStore>,
    /// 社内 SSO(Cloudflare Access の JWT 検証)。`SSO_ENABLED=true` のときだけ Some。
    /// None のときは `/api/v1/auth/sso/login` が 404 を返し、従来のログインだけが使える。
    pub sso: Option<std::sync::Arc<SsoRuntime>>,
}

/// 社内 SSO の実行時の部品。起動時に 1 つ作り、全リクエストで共有する(公開鍵のキャッシュを持つため)。
pub struct SsoRuntime {
    pub verifier: cf_access_jwt::Verifier,
    /// Cookie(`CF_Authorization`)を使うときに、Origin が一致しなければならない、このアプリ自身のオリジン。
    pub allowed_origins: Vec<String>,
}

impl SsoRuntime {
    pub fn new(settings: &SsoSettings) -> anyhow::Result<Self> {
        let verifier = cf_access_jwt::Verifier::try_new(settings.cf.clone())
            .map_err(|e| anyhow::anyhow!("社内 SSO の検証器を初期化できません: {e}"))?;
        Ok(Self {
            verifier,
            allowed_origins: settings.allowed_origins.clone(),
        })
    }
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

    // IMAP設定（受信メール取得用 — EDI_MP email_receiver.py 移植）
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_user: String,
    pub imap_password: String,

    /// 社内 SSO の設定。`SSO_ENABLED=true` のときだけ Some(必須キーが足りなければ起動を止める)。
    pub sso: Option<SsoSettings>,
}

/// 社内 SSO の設定(環境変数から)。
#[derive(Debug, Clone)]
pub struct SsoSettings {
    pub cf: cf_access_jwt::Config,
    pub allowed_origins: Vec<String>,
}

/// SSO が有効なとき必須の環境変数。
const SSO_REQUIRED_KEYS: &[&str] = &[
    "CF_ACCESS_TEAM_DOMAIN",
    "CF_ACCESS_AUD",
    "CF_ACCESS_ALLOWED_EMAIL_DOMAINS",
];

/// `SSO_ENABLED` の値が「有効」か。`true`(大文字小文字を区別しない)だけを有効とする(それ以外は無効=従来のログインだけ)。
fn sso_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
}

/// SSO が有効なのに必須の環境変数が足りない・空のとき、足りないキー名を返す(値は出さない)。
/// 気づかないまま SSO が効かない・ドメイン制限なしで動く、を防ぐために、起動を止める材料にする。
fn missing_sso_keys(get: impl Fn(&str) -> Option<String>) -> Vec<&'static str> {
    SSO_REQUIRED_KEYS
        .iter()
        .copied()
        .filter(|k| get(k).map(|v| v.trim().is_empty()).unwrap_or(true))
        .collect()
}

/// 社内 SSO の設定を環境変数から作る。無効なら None。有効で必須キーが足りなければ、エラーで起動を止める。
fn sso_from_env(base_url: &str) -> anyhow::Result<Option<SsoSettings>> {
    if !sso_enabled(std::env::var("SSO_ENABLED").ok().as_deref()) {
        return Ok(None);
    }
    let missing = missing_sso_keys(|k| std::env::var(k).ok());
    if !missing.is_empty() {
        anyhow::bail!(
            "SSO_ENABLED=true ですが、必須の環境変数が足りません: {}。\
             社内 SSO(Cloudflare Access)の設定を確認してください(値は出しません)",
            missing.join(", ")
        );
    }
    // メールドメインの制限が空だと、Access を通れた誰でも入れてしまう。Service Token は必ず拒否にする(部品が保証)
    let cf = cf_access_jwt::Config::from_env_for_human_login()
        .map_err(|e| anyhow::anyhow!("社内 SSO の設定が不正です: {e}"))?;
    let allowed_origins = {
        let o = base_url.trim().trim_end_matches('/');
        if o.is_empty() { vec![] } else { vec![o.to_string()] }
    };
    Ok(Some(SsoSettings { cf, allowed_origins }))
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
        // 社内 SSO(有効で設定が足りなければ、ここでエラーを返し、起動を止める)
        let sso = sso_from_env(&base_url)?;

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
            sso,
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

#[cfg(test)]
mod sso_config_tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn sso_は_true_と書いたときだけ有効() {
        assert!(sso_enabled(Some("true")));
        assert!(sso_enabled(Some("TRUE")));
        assert!(sso_enabled(Some(" True ")));
        // それ以外は無効(従来のログインだけ)。未設定も無効。
        for v in [None, Some(""), Some("false"), Some("1"), Some("yes"), Some("on")] {
            assert!(!sso_enabled(v), "{v:?}");
        }
    }

    #[test]
    fn 必須キーが全部あれば_足りないものは無い() {
        let get = env(&[
            ("CF_ACCESS_TEAM_DOMAIN", "https://example.cloudflareaccess.com"),
            ("CF_ACCESS_AUD", "aud"),
            ("CF_ACCESS_ALLOWED_EMAIL_DOMAINS", "example.com"),
        ]);
        assert!(missing_sso_keys(get).is_empty());
    }

    #[test]
    fn 必須キーが足りない_または空なら_そのキー名を返す() {
        let get = env(&[("CF_ACCESS_TEAM_DOMAIN", "https://example.cloudflareaccess.com"), ("CF_ACCESS_AUD", "  ")]);
        let missing = missing_sso_keys(get);
        assert_eq!(missing, vec!["CF_ACCESS_AUD", "CF_ACCESS_ALLOWED_EMAIL_DOMAINS"]);
        // 何も無ければ 3 つとも
        assert_eq!(missing_sso_keys(env(&[])).len(), 3);
    }

    #[test]
    fn 足りないキーの通知に_値は含めない() {
        // 値が空でない別のキーの値が、メッセージ(キー名の一覧)に混ざらないこと
        let get = env(&[("CF_ACCESS_AUD", "super-secret-aud")]);
        let joined = missing_sso_keys(get).join(", ");
        assert!(!joined.contains("super-secret-aud"));
    }
}
