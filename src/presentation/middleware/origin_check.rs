/// presentation/middleware/origin_check.rs — CSRF対策（Origin/Refererチェック）の配線
///
/// auth_core::presentation::middleware::origin_check_middleware をラップし、
/// 外部システムからの直接POST（Origin/Refererヘッダーを持たない）を除外パスとして
/// 通過させる。除外パスはStep3 Phase7-1でユーザー承認済み（
/// docs/claude_Step3タスクリスト_Sophia認証方式改修.md 7-1参照）:
/// - `/v1/*`（Peppol企業APIキー認証。api_key_authミドルウェアで別途認証済み）
/// - `/api/v1/peppol/inbound`, `/api/peppol/inbound`（Peppol Webhook、共有シークレット認証）
/// - `/api/v1/webhook/timesheet`, `/api/webhook/timesheet`（GAS連携、HMAC-SHA256認証）
/// - `/api/v1/mail/webhook`, `/api/mail/webhook`（GAS連携、HMAC-SHA256認証。品質改善P1-3で
///   ハンドラ側の認証を実装したのに合わせて追加。従来はここも非除外のままだったため、
///   auth_middlewareのバイパス漏れが直った後もこちらで引き続きブロックされる状態だった）
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// Origin/Refererチェックの除外パスかどうかを判定する
fn is_origin_check_exempt(path: &str) -> bool {
    path.starts_with("/v1/")
        || path == "/api/v1/peppol/inbound"
        || path == "/api/peppol/inbound"
        || path == "/api/v1/webhook/timesheet"
        || path == "/api/webhook/timesheet"
        || path == "/api/v1/mail/webhook"
        || path == "/api/mail/webhook"
}

/// `auth_core::presentation::middleware::origin_check_middleware` に、
/// Sophia固有の除外パス判定を加えたラッパー。
pub fn sophia_origin_check_middleware(
    allowed_origins: Arc<Vec<String>>,
) -> impl Fn(Request, Next) -> BoxFuture<Response> + Clone {
    move |req: Request, next: Next| {
        let allowed = allowed_origins.clone();
        Box::pin(async move {
            if is_origin_check_exempt(req.uri().path()) {
                return next.run(req).await;
            }
            auth_core::presentation::middleware::origin_check_middleware(allowed)(req, next).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exempts_v1_api_key_routes() {
        assert!(is_origin_check_exempt("/v1/orders"));
        assert!(is_origin_check_exempt("/v1/invoices/1/accept"));
    }

    #[test]
    fn exempts_peppol_webhook() {
        assert!(is_origin_check_exempt("/api/v1/peppol/inbound"));
        assert!(is_origin_check_exempt("/api/peppol/inbound"));
    }

    #[test]
    fn exempts_gas_webhook() {
        assert!(is_origin_check_exempt("/api/v1/webhook/timesheet"));
        assert!(is_origin_check_exempt("/api/webhook/timesheet"));
        assert!(is_origin_check_exempt("/api/v1/mail/webhook"));
        assert!(is_origin_check_exempt("/api/mail/webhook"));
    }

    #[test]
    fn does_not_exempt_normal_routes() {
        assert!(!is_origin_check_exempt("/api/v1/orders"));
        assert!(!is_origin_check_exempt("/api/v1/auth/login"));
    }
}
