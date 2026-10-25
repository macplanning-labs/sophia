//! Cloudflare Access が付与する JWT(`Cf-Access-Jwt-Assertion` ヘッダー)を検証する。
//!
//! SSO 配下のアプリは、このクレートで署名・発行者・aud・期限を確認してから
//! メールアドレスを利用者の識別に使う。ヘッダーを検証せずに信じると、
//! Tunnel を迂回されたときに誰にでもなりすませるため、必ずこのクレートを通すこと。
//!
//! 使い方(人のログイン):
//! 1. 起動時に [`Config::from_env_for_human_login`] と [`Verifier::try_new`] で検証器を作る。
//! 2. リクエストごとに [`access_token`] で JWT を取り出す(ヘッダー優先。ヘッダーが無い Bypass の
//!    経路では、Origin が自分のオリジンと完全一致するときだけ `CF_Authorization` Cookie を使う)。
//! 3. [`Verifier::verify`] で検証し、返った email でアプリの利用者を探す。
//!
//! 認証元(Keycloak / Google Workspace)が切り替わっても、ここで返す email は同じになる
//! (小文字に正規化する)。アプリは email だけで利用者を特定し、IdP の種別には依存しない。

mod extract;
mod source;

pub use extract::{CfAccess, CfAccessUser};
pub use source::{access_token, TokenSourceError, COOKIE_NAME};

use std::collections::HashMap;
use std::time::{Duration, Instant};

use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};

/// Cloudflare Access が JWT を載せるヘッダー名。
pub const HEADER_NAME: &str = "cf-access-jwt-assertion";

/// 検証の設定。値は環境変数から読む([`Config::from_env`])。
#[derive(Debug, Clone)]
pub struct Config {
    /// 例: `https://<team>.cloudflareaccess.com`(末尾スラッシュなし)。JWT の `iss` と一致する必要がある。
    pub team_domain: String,
    /// Access アプリケーションの AUD タグ。いずれかと一致すれば受け入れる。
    pub audiences: Vec<String>,
    /// 空でなければ、この中のドメインのメールアドレスだけ受け入れる(小文字)。
    pub allowed_email_domains: Vec<String>,
    /// Service Token(機械用の通行証)による通過を受け入れるか。既定は受け入れない。
    pub allow_service_tokens: bool,
    /// 時刻のずれの許容(秒)。
    pub leeway_secs: u64,
    /// 公開鍵の一覧を取り直す最短間隔。未知の kid が来るたびに取りに行かないため。
    pub jwks_min_refresh: Duration,
    /// 公開鍵の一覧を定期的に取り直す間隔(鍵のローテーション対策)。
    pub jwks_ttl: Duration,
    /// 公開鍵の取得先。通常は `None`(= `{team_domain}/cdn-cgi/access/certs`)。テスト用。
    pub jwks_url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("環境変数 {0} が設定されていません")]
    Missing(&'static str),
    #[error("環境変数 {0} の値が不正です")]
    Invalid(&'static str),
    /// 公開鍵を取りに行く HTTP クライアントを作れなかった(TLS の初期化の失敗など)。
    #[error("HTTP クライアントを初期化できません: {0}")]
    HttpClient(String),
}

impl Config {
    /// 必須: `CF_ACCESS_TEAM_DOMAIN`, `CF_ACCESS_AUD`(カンマ区切り可)。
    /// 任意: `CF_ACCESS_ALLOWED_EMAIL_DOMAINS`(カンマ区切り), `CF_ACCESS_ALLOW_SERVICE_TOKENS`(true/false)。
    pub fn from_env() -> Result<Self, ConfigError> {
        let team_domain = std::env::var("CF_ACCESS_TEAM_DOMAIN")
            .map_err(|_| ConfigError::Missing("CF_ACCESS_TEAM_DOMAIN"))?;
        let audiences = split_csv(
            &std::env::var("CF_ACCESS_AUD").map_err(|_| ConfigError::Missing("CF_ACCESS_AUD"))?,
        );
        let allowed_email_domains = std::env::var("CF_ACCESS_ALLOWED_EMAIL_DOMAINS")
            .map(|v| split_csv(&v.to_ascii_lowercase()))
            .unwrap_or_default();
        let allow_service_tokens = match std::env::var("CF_ACCESS_ALLOW_SERVICE_TOKENS") {
            Ok(v) => {
                parse_bool(&v).ok_or(ConfigError::Invalid("CF_ACCESS_ALLOW_SERVICE_TOKENS"))?
            }
            Err(_) => false,
        };
        Self::new(team_domain, audiences).map(|c| Config {
            allowed_email_domains,
            allow_service_tokens,
            ..c
        })
    }

    /// 人のログイン用の設定を、環境変数から作る。アプリごとに同じ条件を書かないための入口。
    /// [`Config::from_env`] に、[`Config::require_human_login`] の条件を加えたもの。
    pub fn from_env_for_human_login() -> Result<Self, ConfigError> {
        Self::from_env()?.require_human_login()
    }

    /// 人のログイン(アプリの利用者を、メールアドレスで特定する使い方)に必要な条件を満たすようにする。
    ///
    /// - メールドメインの制限が空だと、Access を通れた誰でも入れてしまうため、**空ならエラー**にする
    ///   (気づかないままドメイン制限なしで動くのを防ぐ)。
    /// - Service Token(機械の通行証)は、人としてのログインには使えない。**必ず受け入れない**設定にする
    ///   (`CF_ACCESS_ALLOW_SERVICE_TOKENS=true` が指定されていても上書きする。安全側に倒す)。
    pub fn require_human_login(mut self) -> Result<Self, ConfigError> {
        if self.allowed_email_domains.is_empty() {
            return Err(ConfigError::Missing("CF_ACCESS_ALLOWED_EMAIL_DOMAINS"));
        }
        self.allow_service_tokens = false;
        Ok(self)
    }

    pub fn new(
        team_domain: impl Into<String>,
        audiences: Vec<String>,
    ) -> Result<Self, ConfigError> {
        let team_domain = team_domain.into().trim().trim_end_matches('/').to_string();
        if !team_domain.starts_with("https://") && !team_domain.starts_with("http://127.0.0.1") {
            return Err(ConfigError::Invalid("CF_ACCESS_TEAM_DOMAIN"));
        }
        if audiences.is_empty() {
            return Err(ConfigError::Invalid("CF_ACCESS_AUD"));
        }
        Ok(Config {
            team_domain,
            audiences,
            allowed_email_domains: Vec::new(),
            allow_service_tokens: false,
            leeway_secs: 60,
            jwks_min_refresh: Duration::from_secs(30),
            jwks_ttl: Duration::from_secs(3600),
            jwks_url: None,
        })
    }

    fn certs_url(&self) -> String {
        self.jwks_url
            .clone()
            .unwrap_or_else(|| format!("{}/cdn-cgi/access/certs", self.team_domain))
    }
}

fn split_csv(v: &str) -> Vec<String> {
    v.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" | "" => Some(false),
        _ => None,
    }
}

/// 検証済みの通過者。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Principal {
    /// 人。`email` は小文字に正規化済み。
    User { email: String, sub: String },
    /// Service Token による機械の通過。`common_name` は Client ID。
    ServiceToken { common_name: String },
}

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("JWT がありません")]
    Missing,
    #[error("JWT の形式が不正です")]
    Malformed,
    #[error("許可していない署名方式です")]
    UnsupportedAlgorithm,
    #[error("署名鍵が見つかりません")]
    UnknownKey,
    #[error("JWT の検証に失敗しました: {0}")]
    Invalid(#[from] jsonwebtoken::errors::Error),
    #[error("許可していないメールドメインです")]
    EmailDomainNotAllowed,
    #[error("Service Token は許可されていません")]
    ServiceTokenNotAllowed,
    #[error("利用者を特定できる情報がありません")]
    NoIdentity,
    #[error("公開鍵の取得に失敗しました: {0}")]
    Jwks(String),
}

#[derive(Debug, Deserialize)]
struct Claims {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    common_name: Option<String>,
}

#[derive(Default)]
struct KeyCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Option<Instant>,
}

/// JWT 検証器。公開鍵をキャッシュするので、アプリ全体で 1 つを `Arc` で共有する。
pub struct Verifier {
    cfg: Config,
    http: reqwest::Client,
    cache: RwLock<KeyCache>,
    refresh_lock: Mutex<()>,
}

impl Verifier {
    /// 検証器を作る。HTTP クライアントを作れなければ、エラーを返す(アプリの起動時に使う)。
    pub fn try_new(cfg: Config) -> Result<Self, ConfigError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| ConfigError::HttpClient(e.without_url().to_string()))?;
        Ok(Verifier {
            cfg,
            http,
            cache: RwLock::new(KeyCache::default()),
            refresh_lock: Mutex::new(()),
        })
    }

    /// [`Verifier::try_new`] の、失敗したらプロセスを止める版(既存の呼び出しとの互換のために残す)。
    ///
    /// # Panics
    /// HTTP クライアントを作れないとき(TLS の初期化の失敗など)。**起動時に 1 回だけ**通る場所で使い、
    /// リクエストの処理中には呼ばない。新しいアプリは、エラーを返せる [`Verifier::try_new`] を使う。
    pub fn new(cfg: Config) -> Self {
        Self::try_new(cfg).expect("Access の JWT 検証器の初期化に失敗")
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// JWT を検証し、通過者を返す。
    pub async fn verify(&self, token: &str) -> Result<Principal, VerifyError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(VerifyError::Missing);
        }
        let header = decode_header(token).map_err(|_| VerifyError::Malformed)?;
        // Cloudflare Access は RS256 で署名する。それ以外(none / HS256 など)は一切受け入れない。
        if header.alg != Algorithm::RS256 {
            return Err(VerifyError::UnsupportedAlgorithm);
        }
        let kid = header.kid.ok_or(VerifyError::Malformed)?;
        let key = self.key_for(&kid).await?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&self.cfg.audiences);
        validation.set_issuer(&[self.cfg.team_domain.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.validate_nbf = true;
        validation.leeway = self.cfg.leeway_secs;

        let claims = decode::<Claims>(token, &key, &validation)?.claims;
        self.principal_from(claims)
    }

    fn principal_from(&self, claims: Claims) -> Result<Principal, VerifyError> {
        let email = claims
            .email
            .map(|e| e.trim().to_ascii_lowercase())
            .filter(|e| !e.is_empty());
        if let Some(email) = email {
            if !self.cfg.allowed_email_domains.is_empty() {
                let domain = email.rsplit_once('@').map(|(_, d)| d).unwrap_or("");
                if !self.cfg.allowed_email_domains.iter().any(|d| d == domain) {
                    return Err(VerifyError::EmailDomainNotAllowed);
                }
            }
            return Ok(Principal::User {
                email,
                sub: claims.sub.unwrap_or_default(),
            });
        }
        // Service Token の JWT は email を持たず、common_name に Client ID が入る(sub は空)。
        let common_name = claims.common_name.filter(|c| !c.is_empty());
        let sub_empty = claims.sub.as_deref().unwrap_or("").is_empty();
        match common_name {
            Some(common_name) if sub_empty => {
                if self.cfg.allow_service_tokens {
                    Ok(Principal::ServiceToken { common_name })
                } else {
                    Err(VerifyError::ServiceTokenNotAllowed)
                }
            }
            _ => Err(VerifyError::NoIdentity),
        }
    }

    async fn key_for(&self, kid: &str) -> Result<DecodingKey, VerifyError> {
        {
            let cache = self.cache.read().await;
            let fresh = cache
                .fetched_at
                .is_some_and(|t| t.elapsed() < self.cfg.jwks_ttl);
            if fresh {
                if let Some(k) = cache.keys.get(kid) {
                    return Ok(k.clone());
                }
            }
        }

        // 同時に何件も取りに行かないよう 1 本に絞る。
        let _guard = self.refresh_lock.lock().await;
        {
            let cache = self.cache.read().await;
            let age = cache.fetched_at.map(|t| t.elapsed());
            let within_ttl = age.is_some_and(|a| a < self.cfg.jwks_ttl);
            if within_ttl {
                if let Some(k) = cache.keys.get(kid) {
                    return Ok(k.clone());
                }
                // 未知の kid でも、直前に取り直したばかりなら再取得しない(攻撃で連打されないため)。
                if age.is_some_and(|a| a < self.cfg.jwks_min_refresh) {
                    return Err(VerifyError::UnknownKey);
                }
            }
        }

        let keys = match self.fetch_keys().await {
            Ok(keys) => keys,
            Err(e) => {
                // 取得に失敗しても、TTL 切れの古い鍵で検証を続ける(一時的な取得失敗で全員を締め出さない)。
                let cache = self.cache.read().await;
                if let Some(k) = cache.keys.get(kid) {
                    tracing::warn!(error = %e, "Access の公開鍵の再取得に失敗。キャッシュ済みの鍵で検証を続けます");
                    return Ok(k.clone());
                }
                return Err(e);
            }
        };
        let mut cache = self.cache.write().await;
        cache.keys = keys;
        cache.fetched_at = Some(Instant::now());
        cache.keys.get(kid).cloned().ok_or(VerifyError::UnknownKey)
    }

    async fn fetch_keys(&self) -> Result<HashMap<String, DecodingKey>, VerifyError> {
        let resp = self
            .http
            .get(self.cfg.certs_url())
            .send()
            .await
            .map_err(|e| VerifyError::Jwks(e.without_url().to_string()))?;
        if !resp.status().is_success() {
            return Err(VerifyError::Jwks(format!("HTTP {}", resp.status())));
        }
        let set: JwkSet = resp
            .json()
            .await
            .map_err(|e| VerifyError::Jwks(e.without_url().to_string()))?;
        let mut keys = HashMap::new();
        for jwk in set.keys {
            if !matches!(jwk.algorithm, AlgorithmParameters::RSA(_)) {
                continue;
            }
            let Some(kid) = jwk.common.key_id.clone() else {
                continue;
            };
            match DecodingKey::from_jwk(&jwk) {
                Ok(k) => {
                    keys.insert(kid, k);
                }
                Err(e) => tracing::warn!(error = %e, "Access の公開鍵を読み込めませんでした"),
            }
        }
        if keys.is_empty() {
            return Err(VerifyError::Jwks("RSA 鍵がありません".into()));
        }
        Ok(keys)
    }
}
