/// presentation/middleware/origin_check.rs — CSRF対策（Origin/Refererチェック）の配線
///
/// Origin/Referer の検査を、このファイルで完全一致により行う。
/// （共有部品 auth_core の origin_check_middleware は前方一致のため、
///  `https://許可ドメイン.evil.com` を通してしまうので使わない。DEMO-000229）
///
/// 外部システムからの直接POST（Origin/Refererヘッダーを持たない）を除外パスとして
/// 通過させる。除外パスはStep3 Phase7-1でユーザー承認済み（
/// docs/claude_Step3タスクリスト_Sophia認証方式改修.md 7-1参照）:
/// - `/v1/*`（Peppol企業APIキー認証。api_key_authミドルウェアで別途認証済み）
/// - `/api/v1/peppol/inbound`, `/api/peppol/inbound`（Peppol Webhook、共有シークレット認証）
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Json, Response};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

const REASON_MISMATCH: &str = "Origin/Refererが許可リストに含まれていません";
const REASON_MISSING: &str = "Origin/Refererヘッダーがありません";

/// Origin/Refererチェックの除外パスかどうかを判定する
fn is_origin_check_exempt(path: &str) -> bool {
    path.starts_with("/v1/") || path == "/api/v1/peppol/inbound" || path == "/api/peppol/inbound"
}

/// 許可リストを正規化する: 前後の空白を除き、末尾の `/` を除く。空になった要素は捨てる。
fn normalize_allowed(allowed: &[String]) -> Vec<String> {
    allowed
        .iter()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Refererの origin 部分を取り出す。`://` が無ければ None(不正)。
/// `://` の後で最初に現れる `/`・`?`・`#` の手前まで。無ければ全体。
fn referer_origin(referer: &str) -> Option<&str> {
    let scheme_end = referer.find("://")?;
    let authority_start = scheme_end + "://".len();
    let rest = &referer[authority_start..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    Some(&referer[..authority_start + end])
}

/// Origin/Referer の判定(純関数)。
/// - Origin があれば、Origin だけで判定する(Referer は見ない)
/// - Origin が無ければ Referer の origin 部分で判定する
/// - 比較は許可リストとの完全一致(ASCII 大文字小文字は無視)
pub(crate) fn origin_allowed(
    allowed: &[String],
    origin: Option<&str>,
    referer: Option<&str>,
) -> Result<(), &'static str> {
    let allowed = normalize_allowed(allowed);
    let matches = |candidate: &str| {
        candidate != "null" && allowed.iter().any(|a| a.eq_ignore_ascii_case(candidate))
    };

    if let Some(origin) = origin {
        return if matches(origin) {
            Ok(())
        } else {
            Err(REASON_MISMATCH)
        };
    }

    match referer {
        Some(referer) => match referer_origin(referer) {
            Some(origin) if matches(origin) => Ok(()),
            _ => Err(REASON_MISMATCH),
        },
        None => Err(REASON_MISSING),
    }
}

/// 403 応答(共有部品と同じ形: `{"error":"origin_check_failed","detail":"<理由>"}`)
fn forbidden(reason: &'static str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "error": "origin_check_failed",
            "detail": reason,
        })),
    )
        .into_response()
}

/// 許可された origin からの状態変更リクエストだけを通す CSRF 対策ミドルウェア。
/// Sophia 固有の除外パスと、安全なメソッド(GET/HEAD/OPTIONS)は素通しする。
pub fn sophia_origin_check_middleware(
    allowed_origins: Arc<Vec<String>>,
) -> impl Fn(Request, Next) -> BoxFuture<Response> + Clone {
    move |req: Request, next: Next| {
        let allowed = allowed_origins.clone();
        Box::pin(async move {
            if is_origin_check_exempt(req.uri().path())
                || matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS)
            {
                return next.run(req).await;
            }

            // UTF-8 として読めない値は、空文字として扱う(空文字は必ず不一致になる)。
            let origin = req
                .headers()
                .get(header::ORIGIN)
                .map(|v| v.to_str().unwrap_or(""));
            let referer = req
                .headers()
                .get(header::REFERER)
                .map(|v| v.to_str().unwrap_or(""));

            match origin_allowed(&allowed, origin, referer) {
                Ok(()) => next.run(req).await,
                Err(reason) => {
                    // 値(Origin/Referer)はログに出さない。理由の種類だけ出す。
                    tracing::warn!(reason, "origin check failed");
                    forbidden(reason)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

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
    fn does_not_exempt_normal_routes() {
        assert!(!is_origin_check_exempt("/api/v1/orders"));
        assert!(!is_origin_check_exempt("/api/v1/auth/login"));
        // 社内 SSO のログインも、Origin 検査の除外にしない(POST。Cookie 経路は Origin 完全一致が前提)
        assert!(!is_origin_check_exempt("/api/v1/auth/sso/login"));
    }

    // ---- origin_allowed(純関数) ----

    const ALLOWED: &str = "https://sophia-app.macplanning.com";

    #[test]
    fn origin_exact_match_passes() {
        assert_eq!(
            origin_allowed(&list(&[ALLOWED]), Some(ALLOWED), None),
            Ok(())
        );
    }

    #[test]
    fn origin_case_insensitive_passes() {
        assert_eq!(
            origin_allowed(
                &list(&[ALLOWED]),
                Some("HTTPS://SOPHIA-APP.MACPLANNING.COM"),
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn origin_prefix_spoof_rejected() {
        assert_eq!(
            origin_allowed(
                &list(&[ALLOWED]),
                Some("https://sophia-app.macplanning.com.evil.com"),
                None
            ),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn origin_port_or_scheme_difference_rejected() {
        assert_eq!(
            origin_allowed(
                &list(&[ALLOWED]),
                Some("https://sophia-app.macplanning.com:8443"),
                None
            ),
            Err(REASON_MISMATCH)
        );
        assert_eq!(
            origin_allowed(
                &list(&[ALLOWED]),
                Some("http://sophia-app.macplanning.com"),
                None
            ),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn empty_entry_in_allowed_list_does_not_allow_unrelated_origin() {
        assert_eq!(
            origin_allowed(
                &list(&["", "https://a.example"]),
                Some("https://evil.example"),
                None
            ),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn all_empty_allowed_list_rejects_everything() {
        assert_eq!(
            origin_allowed(&list(&["", "   "]), Some("https://evil.example"), None),
            Err(REASON_MISMATCH)
        );
        assert_eq!(
            origin_allowed(&list(&[]), Some("https://evil.example"), None),
            Err(REASON_MISMATCH)
        );
        assert_eq!(
            origin_allowed(&list(&[]), None, Some("https://evil.example/page")),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn trailing_slash_in_allowed_list_matches() {
        assert_eq!(
            origin_allowed(
                &list(&["https://a.example/"]),
                Some("https://a.example"),
                None
            ),
            Ok(())
        );
    }

    #[test]
    fn referer_with_path_and_query_passes() {
        assert_eq!(
            origin_allowed(
                &list(&["https://a.example"]),
                None,
                Some("https://a.example/page?x=1")
            ),
            Ok(())
        );
    }

    #[test]
    fn referer_userinfo_trick_rejected() {
        assert_eq!(
            origin_allowed(
                &list(&["https://a.example"]),
                None,
                Some("https://a.example@evil.com/")
            ),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn referer_subdomain_suffix_rejected() {
        assert_eq!(
            origin_allowed(
                &list(&["https://a.example"]),
                None,
                Some("https://a.example.evil.com/x")
            ),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn referer_without_scheme_rejected() {
        assert_eq!(
            origin_allowed(&list(&["https://a.example"]), None, Some("a.example/page")),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn origin_mismatch_does_not_fall_back_to_matching_referer() {
        assert_eq!(
            origin_allowed(
                &list(&["https://a.example"]),
                Some("https://evil.example"),
                Some("https://a.example/page"),
            ),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn origin_null_rejected() {
        assert_eq!(
            origin_allowed(&list(&["https://a.example"]), Some("null"), None),
            Err(REASON_MISMATCH)
        );
    }

    #[test]
    fn neither_origin_nor_referer_rejected() {
        assert_eq!(
            origin_allowed(&list(&["https://a.example"]), None, None),
            Err(REASON_MISSING)
        );
    }

    // ---- ルーターを通すテスト ----

    async fn ok_handler() -> &'static str {
        "ok"
    }

    fn app(allowed: &[&str]) -> Router {
        Router::new()
            .route("/api/v1/orders", post(ok_handler).get(ok_handler))
            .route("/v1/orders", post(ok_handler))
            .route("/api/v1/peppol/inbound", post(ok_handler))
            .route("/api/v1/auth/sso/login", post(ok_handler))
            .layer(axum::middleware::from_fn(sophia_origin_check_middleware(
                Arc::new(list(allowed)),
            )))
    }

    async fn send(app: Router, req: HttpRequest<Body>) -> StatusCode {
        app.oneshot(req).await.expect("応答").status()
    }

    fn req(method: Method, path: &str, origin: Option<&str>) -> HttpRequest<Body> {
        let mut b = HttpRequest::builder().method(method).uri(path);
        if let Some(o) = origin {
            b = b.header(header::ORIGIN, o);
        }
        b.body(Body::empty()).expect("request")
    }

    #[tokio::test]
    async fn router_allows_matching_origin_and_rejects_spoofs() {
        let allowed = ["https://a.example"];
        assert_eq!(
            send(
                app(&allowed),
                req(Method::POST, "/api/v1/orders", Some("https://a.example"))
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            send(
                app(&allowed),
                req(Method::POST, "/api/v1/orders", Some("https://evil.example"))
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send(
                app(&allowed),
                req(
                    Method::POST,
                    "/api/v1/orders",
                    Some("https://a.example.evil.com")
                )
            )
            .await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn router_get_passes_without_headers() {
        assert_eq!(
            send(
                app(&["https://a.example"]),
                req(Method::GET, "/api/v1/orders", None)
            )
            .await,
            StatusCode::OK
        );
        assert_eq!(
            send(
                app(&["https://a.example"]),
                req(Method::GET, "/api/v1/auth/sso/login", None)
            )
            .await,
            StatusCode::METHOD_NOT_ALLOWED
        );
    }

    #[tokio::test]
    async fn router_exempt_paths_pass_without_headers() {
        let allowed = ["https://a.example"];
        assert_eq!(
            send(app(&allowed), req(Method::POST, "/v1/orders", None)).await,
            StatusCode::OK
        );
        assert_eq!(
            send(
                app(&allowed),
                req(Method::POST, "/api/v1/peppol/inbound", None)
            )
            .await,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn router_sso_login_without_headers_is_forbidden() {
        assert_eq!(
            send(
                app(&["https://a.example"]),
                req(Method::POST, "/api/v1/auth/sso/login", None)
            )
            .await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn router_empty_allowed_entry_rejects_any_origin() {
        assert_eq!(
            send(
                app(&[""]),
                req(Method::POST, "/api/v1/orders", Some("https://evil.example"))
            )
            .await,
            StatusCode::FORBIDDEN
        );
    }
}
