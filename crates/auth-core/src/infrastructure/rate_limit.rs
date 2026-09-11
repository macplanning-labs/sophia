//! infrastructure/rate_limit.rs — IPレート制限（①）＋ アカウント試行ロック（②）
//!
//! 方針ドキュメント1.2節の統合設計に対応する実装。
//! - ①`ip_rate_limit_layer`: `tower_governor`（`SmartIpKeyExtractor`）を
//!   純粋な`tower::Layer`としてラップしたもの。ルーター全体に一括適用する。
//!   WIPの`rate_limiter.rs`から移植・一般化した。
//! - ②`attempt_lock_middleware`: アカウント（メールアドレス等）単位の試行回数
//!   ロック。`axum::middleware::from_fn`ベースで、ログイン系ルートに
//!   `route_layer`として個別適用する。Sophiaの`login_guard.rs`相当の新規実装。

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
    key_extractor::{KeyExtractor, SmartIpKeyExtractor},
    GovernorLayer,
};

use crate::domain::attempt_lock::{
    build_lock_key, AttemptLockConfig, AttemptOutcome, AttemptOutcomeKind, AttemptStore, LockKey,
};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

// ---------------------------------------------------------------------------
// ① IPトークンバケット（tower_governor、ネットワーク層）
// ---------------------------------------------------------------------------

pub type CoreGovernorConfig = GovernorConfig<SmartIpKeyExtractor, NoOpMiddleware>;

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
    // 重要: key_extractorを明示的にSmartIpKeyExtractorにしない場合、
    // デフォルトのPeerIpKeyExtractorはリバースプロキシのコンテナIPしか
    // 見られず、実質「全クライアント共有のグローバル制限」になってしまう。
    // 必ずSmartIpKeyExtractorを使うこと（X-Forwarded-For等のヘッダーを優先し、
    // 無ければpeer IPにフォールバックする。リバースプロキシ側でヘッダーが
    // 正しく設定されていることが前提）。
    let mut builder = GovernorConfigBuilder::default().key_extractor(SmartIpKeyExtractor);
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
) -> GovernorLayer<SmartIpKeyExtractor, NoOpMiddleware, Body> {
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
    move |req: Request, next: Next| {
        let store = store.clone();
        let outcome = outcome.clone();
        Box::pin(run_attempt_lock::<T>(req, next, store, outcome, cfg))
    }
}

async fn run_attempt_lock<T>(
    req: Request,
    next: Next,
    store: Arc<dyn AttemptStore>,
    outcome: Arc<dyn AttemptOutcome>,
    cfg: AttemptLockConfig,
) -> Response
where
    T: DeserializeOwned + LockKey + Send + 'static,
{
    // IPはSmartIpKeyExtractorで抽出する(①と同じロジックを再利用し、
    // X-Forwarded-For等のヘッダー優先・peer IPフォールバックの実装を重複させない)。
    // ボディを読む前に行うため、リクエストの所有権はまだ奪わない。
    let ip = match SmartIpKeyExtractor.extract(&req) {
        Ok(ip) => ip.to_string(),
        Err(_) => return internal_error_response("レート制限の判定に失敗しました"),
    };

    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, cfg.max_body_bytes).await {
        Ok(b) => b,
        Err(_) => return payload_too_large_response(cfg.max_body_bytes),
    };

    let lock_key = serde_json::from_slice::<T>(&bytes)
        .ok()
        .map(|parsed| build_lock_key(&ip, parsed.lock_key()));

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
