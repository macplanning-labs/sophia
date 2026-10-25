//! infrastructure/rate_limit.rs — IPレート制限（①）＋ アカウント試行ロック（②）
//!
//! 方針ドキュメント1.2節の統合設計に対応する実装。
//! - ①`ip_rate_limit_layer`: `tower_governor`（送り元 IP は `client_ip::TrustedProxyIpKeyExtractor`）を
//!   純粋な`tower::Layer`としてラップしたもの。ルーター全体に一括適用する。
//!   WIPの`rate_limiter.rs`から移植・一般化した。
//! - ②`attempt_lock_middleware`: アカウント（メールアドレス等）単位の試行回数
//!   ロック。`axum::middleware::from_fn`ベースで、ログイン系ルートに
//!   `route_layer`として個別適用する。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use governor::middleware::NoOpMiddleware;
use serde::de::DeserializeOwned;
use serde_json::json;
use tower_governor::{
    errors::GovernorError,
    governor::{GovernorConfig, GovernorConfigBuilder},
    key_extractor::KeyExtractor,
    GovernorLayer,
};

use crate::domain::attempt_lock::{
    build_lock_key, build_scoped_lock_key, AttemptLockConfig, AttemptOutcome, AttemptOutcomeKind,
    AttemptStore, LockKey, LockScope,
};
use crate::infrastructure::client_ip::TrustedProxyIpKeyExtractor;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

// ---------------------------------------------------------------------------
// ① IPトークンバケット（tower_governor、ネットワーク層）
// ---------------------------------------------------------------------------

pub type CoreGovernorConfig = GovernorConfig<TrustedProxyIpKeyExtractor, NoOpMiddleware>;

#[derive(Debug, Clone, Copy)]
pub struct IpRateLimitConfig {
    pub period: StdDuration,
    pub burst_size: u32,
}

impl IpRateLimitConfig {
    pub fn new(period: StdDuration, burst_size: u32) -> Self {
        Self { period, burst_size }
    }

    /// WIP現行のログイン向け既定値: IPごとに20回/分(バースト20、3秒に1回補充)。
    pub fn login_preset() -> Self {
        Self::new(StdDuration::from_secs(3), 20)
    }

    /// 新規登録向け: IPごとに10回/分(バースト10、6秒に1回補充)。
    /// 登録は毎回パスワードのハッシュ計算(重い処理)をするため、ログインより狭くする(DEMO-000153)。
    pub fn register_preset() -> Self {
        Self::new(StdDuration::from_secs(6), 10)
    }

    /// トークン更新向け: IPごとに30回/分(バースト30、2秒に1回補充)。
    /// 通常の端末はアクセストークンの期限ごとに1回更新する程度なので、社内 NAT の内側に
    /// 利用者が多くても届かない広さにしてある(DEMO-000153)。
    pub fn token_refresh_preset() -> Self {
        Self::new(StdDuration::from_secs(2), 30)
    }

    /// WIP現行のWebhook向け既定値: IPごとに60回/分。
    pub fn webhook_preset() -> Self {
        Self::new(StdDuration::from_secs(1), 60)
    }

    /// WIP現行の外部API向け既定値: IPごとに300回/分。
    pub fn external_api_preset() -> Self {
        Self::new(StdDuration::from_millis(200), 300)
    }
}

fn build_governor_config(cfg: IpRateLimitConfig) -> CoreGovernorConfig {
    // key_extractor()はKの型そのものを差し替えるため&mut Selfではなく
    // 新しいowned builderを返す。先にkey_extractorを確定させてから、
    // period/burst_sizeを続けてチェインする。
    //
    // 重要: デフォルトのPeerIpKeyExtractorはリバースプロキシのコンテナIPしか
    // 見られず、実質「全クライアント共有のグローバル制限」になってしまう。
    // 一方、tower_governorのSmartIpKeyExtractorはX-Forwarded-Forの先頭を誰からでも
    // 信用するため、送り手が書いた偽のIPで制限をすり抜けられた(DEMO-000144)。
    // TrustedProxyIpKeyExtractorは、信用する中継から来たときだけ中継が書いた値を使う。
    let mut builder = GovernorConfigBuilder::default().key_extractor(TrustedProxyIpKeyExtractor);
    builder.period(cfg.period).burst_size(cfg.burst_size);
    builder
        .finish()
        .expect("rate limiter config: burst_size/periodが不正です")
}

/// 429応答を共通のエラー規約(`{"detail": "..."}"`)に整形する。
pub fn ip_rate_limit_error_response(error: GovernorError) -> Response {
    let (status, detail) = match error {
        GovernorError::TooManyRequests { wait_time, .. } => (
            StatusCode::TOO_MANY_REQUESTS,
            format!("リクエストが多すぎます。{wait_time}秒後に再試行してください。"),
        ),
        GovernorError::UnableToExtractKey => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "レート制限の判定に失敗しました".to_string(),
        ),
        GovernorError::Other { code, msg, .. } => (
            code,
            msg.unwrap_or_else(|| "リクエストを処理できませんでした".to_string()),
        ),
    };
    (status, Json(json!({ "detail": detail }))).into_response()
}

/// IPトークンバケットのLayerを構築する。ルーター全体に`.layer()`で被せるだけで
/// ハンドラ側は一切関知しない（方針ドキュメント1.2節①）。
///
/// ```ignore
/// let app = Router::new()
///     .merge(routes)
///     .layer(auth_core::infrastructure::rate_limit::ip_rate_limit_layer(
///         auth_core::infrastructure::rate_limit::IpRateLimitConfig::login_preset(),
///     ));
/// ```
pub fn ip_rate_limit_layer(
    cfg: IpRateLimitConfig,
) -> GovernorLayer<TrustedProxyIpKeyExtractor, NoOpMiddleware, Body> {
    GovernorLayer::new(build_governor_config(cfg)).error_handler(ip_rate_limit_error_response)
}

// ---------------------------------------------------------------------------
// ② アカウント単位の試行回数ロック（アプリケーション層）
// ---------------------------------------------------------------------------

fn account_locked_response(retry_after_seconds: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({ "error": "account_locked", "retry_after_seconds": retry_after_seconds })),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

fn payload_too_large_response(limit_bytes: usize) -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({ "error": "payload_too_large", "limit_bytes": limit_bytes })),
    )
        .into_response()
}

fn internal_error_response(detail: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "internal_error", "detail": detail })),
    )
        .into_response()
}

/// アカウント単位の試行ロックミドルウェアを構築する。
///
/// 前提条件: このミドルウェアは必ずAxum標準の`DefaultBodyLimit`レイヤーより
/// 内側（後段）で評価される位置に配置すること（利用側アプリの責務）。
/// 呼び出し側の設定漏れに備え、内部でも`cfg.max_body_bytes`による上限チェックを行う
/// （方針ドキュメント1.2.1節）。
///
/// `T`はログイン系DTO（`LockKey`を実装したフォーム/JSON型）。ボディがJSONとして
/// `T`にデシリアライズできない場合はキー抽出をスキップしてそのまま後段へ流す
/// （＝ロック判定は行わないが、リクエスト自体は拒否しない。不正なボディの拒否は
/// ハンドラ側のバリデーションに委ねる）。
///
/// ```ignore
/// Router::new().route(
///     "/api/auth/login",
///     post(login_handler).route_layer(middleware::from_fn(
///         auth_core::infrastructure::rate_limit::attempt_lock_middleware::<LoginForm>(
///             store.clone(),
///             std::sync::Arc::new(auth_core::domain::attempt_lock::DefaultAttemptOutcome),
///             cfg.clone(),
///         ),
///     )),
/// );
/// ```
pub fn attempt_lock_middleware<T>(
    store: Arc<dyn AttemptStore>,
    outcome: Arc<dyn AttemptOutcome>,
    cfg: AttemptLockConfig,
) -> impl Fn(Request, Next) -> BoxFuture<Response> + Clone
where
    T: DeserializeOwned + LockKey + Send + 'static,
{
    // 従来どおり: "IP:identifier"(識別子はそのまま)。既存の利用側の挙動を変えないため残す。
    move |req: Request, next: Next| {
        let store = store.clone();
        let outcome = outcome.clone();
        Box::pin(run_attempt_lock::<T>(req, next, store, outcome, cfg, None))
    }
}

/// 数える単位（`LockScope`）を指定できる試行ロックミドルウェア（DEMO-000153）。
///
/// `attempt_lock_middleware` との違い:
/// - `LockScope::IdentifierOnly` を選ぶと、IP を問わずアカウント単位で数える
///   （IP を次々に変える分散型の総当たり向け。回数は高めにすること）。
/// - 識別子は前後の空白を除き小文字にそろえてからキーにする（`normalize_lock_identifier`）。
///
/// 1つのルートに両方の単位をかけるときは、`route_layer` を2回重ね、
/// **ストアは単位ごとに別のインスタンス**を渡すこと（数が混ざらないように）。
/// 前提条件（`DefaultBodyLimit` より内側に置く等）は `attempt_lock_middleware` と同じ。
///
/// ```ignore
/// post(login_handler)
///     .route_layer(middleware::from_fn(attempt_lock_middleware_with_scope::<LoginForm>(
///         ip_account_store, outcome.clone(), AttemptLockConfig::default(), LockScope::IpAndIdentifier,
///     )))
///     .route_layer(middleware::from_fn(attempt_lock_middleware_with_scope::<LoginForm>(
///         account_store, outcome, AttemptLockConfig::account_wide_default(), LockScope::IdentifierOnly,
///     )))
/// ```
pub fn attempt_lock_middleware_with_scope<T>(
    store: Arc<dyn AttemptStore>,
    outcome: Arc<dyn AttemptOutcome>,
    cfg: AttemptLockConfig,
    scope: LockScope,
) -> impl Fn(Request, Next) -> BoxFuture<Response> + Clone
where
    T: DeserializeOwned + LockKey + Send + 'static,
{
    move |req: Request, next: Next| {
        let store = store.clone();
        let outcome = outcome.clone();
        Box::pin(run_attempt_lock::<T>(
            req,
            next,
            store,
            outcome,
            cfg,
            Some(scope),
        ))
    }
}

/// `scope` が `None` のときは従来の `attempt_lock_middleware` と同じキー(識別子をそろえない)。
async fn run_attempt_lock<T>(
    req: Request,
    next: Next,
    store: Arc<dyn AttemptStore>,
    outcome: Arc<dyn AttemptOutcome>,
    cfg: AttemptLockConfig,
    scope: Option<LockScope>,
) -> Response
where
    T: DeserializeOwned + LockKey + Send + 'static,
{
    // IPは①と同じTrustedProxyIpKeyExtractorで抽出する(判定を重複させない。
    // 送り手が書いたX-Forwarded-Forの先頭は使わない。DEMO-000144)。
    // ボディを読む前に行うため、リクエストの所有権はまだ奪わない。
    let ip = match TrustedProxyIpKeyExtractor.extract(&req) {
        Ok(ip) => ip.to_string(),
        Err(_) => return internal_error_response("レート制限の判定に失敗しました"),
    };

    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, cfg.max_body_bytes).await {
        Ok(b) => b,
        Err(_) => return payload_too_large_response(cfg.max_body_bytes),
    };

    // JSON の API と、ログイン画面のフォーム(URL エンコード)の両方から識別子を読む。
    // どちらでも読めなければ、ロックの判定はせずに後段へ流す(拒否はハンドラに任せる)
    let lock_key = serde_json::from_slice::<T>(&bytes)
        .ok()
        .or_else(|| serde_urlencoded::from_bytes::<T>(&bytes).ok())
        .map(|parsed| match scope {
            Some(scope) => build_scoped_lock_key(scope, &ip, parsed.lock_key()),
            None => build_lock_key(&ip, parsed.lock_key()),
        });

    if let Some(ref key) = lock_key {
        match store.locked_retry_after_seconds(key).await {
            Ok(Some(retry_after)) => return account_locked_response(retry_after),
            Ok(None) => {}
            Err(_) => return internal_error_response("試行回数の確認に失敗しました"),
        }
    }

    let rebuilt = Request::from_parts(parts, Body::from(bytes));
    let response = next.run(rebuilt).await;

    if let Some(key) = lock_key {
        match outcome.classify(response.status()) {
            Some(AttemptOutcomeKind::Failure) => {
                let _ = store.record_failure(&key, &cfg).await;
            }
            Some(AttemptOutcomeKind::Success) => {
                let _ = store.reset(&key).await;
            }
            None => {}
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_preset_matches_wip_current_config() {
        let cfg = IpRateLimitConfig::login_preset();
        assert_eq!(cfg.period, StdDuration::from_secs(3));
        assert_eq!(cfg.burst_size, 20);
    }

    /// 公開の IP から直接つながった送り手が、X-Forwarded-For の先頭を毎回変えても、
    /// 同じ送り手として数えられ、上限で 429 になる(DEMO-000144 の再発防止)。
    #[tokio::test]
    async fn spoofed_forwarded_for_cannot_bypass_ip_rate_limit() {
        use axum::{routing::get, Router};
        use std::net::SocketAddr;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/login", get(|| async { "ok" }))
            .layer(ip_rate_limit_layer(IpRateLimitConfig::new(
                StdDuration::from_secs(60),
                2,
            )));
        let mut statuses = Vec::new();
        for i in 0..4 {
            let mut req = Request::builder()
                .uri("/login")
                .header("x-forwarded-for", format!("10.9.8.{i}"))
                .header("x-real-ip", format!("10.7.6.{i}"))
                .body(Body::empty())
                .unwrap();
            req.extensions_mut()
                .insert(axum::extract::ConnectInfo(SocketAddr::from((
                    [203, 0, 113, 9],
                    40000,
                ))));
            statuses.push(app.clone().oneshot(req).await.unwrap().status());
        }
        assert_eq!(
            statuses,
            vec![
                StatusCode::OK,
                StatusCode::OK,
                StatusCode::TOO_MANY_REQUESTS,
                StatusCode::TOO_MANY_REQUESTS
            ]
        );
    }

    /// 信用する中継(同じ Docker のネットワークの nginx)からなら、中継が書いた送り元ごとに数える。
    #[tokio::test]
    async fn trusted_proxy_counts_each_client_separately() {
        use axum::{routing::get, Router};
        use std::net::SocketAddr;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/login", get(|| async { "ok" }))
            .layer(ip_rate_limit_layer(IpRateLimitConfig::new(
                StdDuration::from_secs(60),
                1,
            )));
        for client in ["198.51.100.1", "198.51.100.2", "198.51.100.3"] {
            let mut req = Request::builder()
                .uri("/login")
                .header("x-real-ip", client)
                .body(Body::empty())
                .unwrap();
            req.extensions_mut()
                .insert(axum::extract::ConnectInfo(SocketAddr::from((
                    [172, 18, 0, 5],
                    40000,
                ))));
            assert_eq!(
                app.clone().oneshot(req).await.unwrap().status(),
                StatusCode::OK
            );
        }
    }

    // --- アカウント単位の試行ロック(DEMO-000153) ---

    #[derive(serde::Deserialize)]
    struct TestLogin {
        username: String,
    }
    impl LockKey for TestLogin {
        fn lock_key(&self) -> &str {
            &self.username
        }
    }

    /// パスワードが "ok" なら 200、それ以外は 401 を返すだけのログイン(DB 不要)
    async fn stub_login(axum::Json(v): axum::Json<serde_json::Value>) -> StatusCode {
        if v.get("password").and_then(|p| p.as_str()) == Some("ok") {
            StatusCode::OK
        } else {
            StatusCode::UNAUTHORIZED
        }
    }

    fn scoped_app(scope: LockScope, cfg: AttemptLockConfig) -> axum::Router {
        use crate::infrastructure::attempt_store::InMemoryAttemptStore;
        use axum::routing::post;
        let store: Arc<dyn AttemptStore> = Arc::new(InMemoryAttemptStore::new());
        let outcome: Arc<dyn AttemptOutcome> =
            Arc::new(crate::domain::attempt_lock::DefaultAttemptOutcome);
        axum::Router::new().route(
            "/login",
            post(stub_login).route_layer(axum::middleware::from_fn(
                attempt_lock_middleware_with_scope::<TestLogin>(store, outcome, cfg, scope),
            )),
        )
    }

    /// 信用する中継(nginx)経由で、`client` から来たログイン要求
    fn login_req(client: &str, username: &str, password: &str) -> Request {
        let mut req = Request::builder()
            .method("POST")
            .uri("/login")
            .header("content-type", "application/json")
            .header("x-real-ip", client)
            .body(Body::from(
                json!({ "username": username, "password": password }).to_string(),
            ))
            .unwrap();
        req.extensions_mut()
            .insert(axum::extract::ConnectInfo(std::net::SocketAddr::from((
                [172, 18, 0, 5],
                40000,
            ))));
        req
    }

    #[tokio::test]
    async fn ip_and_identifier_scope_normalizes_and_isolates_ips() {
        use tower::ServiceExt;
        let app = scoped_app(LockScope::IpAndIdentifier, AttemptLockConfig::default());
        // 大文字・空白を変えても同じアカウントとして数える
        for name in ["Alice", "alice ", " ALICE", "alice", "aLiCe"] {
            let res = app
                .clone()
                .oneshot(login_req("198.51.100.1", name, "bad"))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        }
        let res = app
            .clone()
            .oneshot(login_req("198.51.100.1", "alice", "ok"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        // 別の IP からの本人は締め出されない
        let res = app
            .clone()
            .oneshot(login_req("198.51.100.2", "alice", "ok"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn identifier_only_scope_locks_across_ips() {
        use tower::ServiceExt;
        let cfg = AttemptLockConfig {
            max_attempts: 3,
            ..AttemptLockConfig::default()
        };
        let app = scoped_app(LockScope::IdentifierOnly, cfg);
        for i in 0..3 {
            let res = app
                .clone()
                .oneshot(login_req(&format!("198.51.100.{i}"), "bob", "bad"))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        }
        let res = app
            .clone()
            .oneshot(login_req("203.0.113.50", "BOB", "ok"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(res.headers().get(axum::http::header::RETRY_AFTER).is_some());
    }

    #[tokio::test]
    async fn legacy_middleware_keeps_exact_identifier() {
        use crate::infrastructure::attempt_store::InMemoryAttemptStore;
        use axum::routing::post;
        use tower::ServiceExt;
        let store: Arc<dyn AttemptStore> = Arc::new(InMemoryAttemptStore::new());
        let outcome: Arc<dyn AttemptOutcome> =
            Arc::new(crate::domain::attempt_lock::DefaultAttemptOutcome);
        let app = axum::Router::new().route(
            "/login",
            post(stub_login).route_layer(axum::middleware::from_fn(attempt_lock_middleware::<
                TestLogin,
            >(
                store,
                outcome,
                AttemptLockConfig::default(),
            ))),
        );
        // 従来の関数は識別子をそろえない(既存の利用側の挙動を変えない)
        for name in ["Carol", "carol", "CAROL", "caRol", "carOl"] {
            app.clone()
                .oneshot(login_req("198.51.100.1", name, "bad"))
                .await
                .unwrap();
        }
        let res = app
            .clone()
            .oneshot(login_req("198.51.100.1", "carol", "ok"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[test]
    fn register_and_refresh_presets() {
        let r = IpRateLimitConfig::register_preset();
        assert_eq!((r.period, r.burst_size), (StdDuration::from_secs(6), 10));
        let t = IpRateLimitConfig::token_refresh_preset();
        assert_eq!((t.period, t.burst_size), (StdDuration::from_secs(2), 30));
    }

    #[test]
    fn account_locked_response_sets_retry_after_header() {
        let response = account_locked_response(823);
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .unwrap(),
            "823"
        );
    }
}
