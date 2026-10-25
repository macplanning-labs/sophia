//! presentation/middleware.rs — 認証保護ルート用の横断的ミドルウェア
//!
//! `AuthUser<U>`（extractors.rs）が個々のハンドラでの認証情報取得を担うのに対し、
//! こちらはリクエスト全体に横断的に適用するミドルウェアを置く。
//!
//! Step 1では、方針ドキュメント6章で挙げられている「状態変更リクエスト
//! （POST/PUT/DELETE）にOrigin/Refererヘッダー検証を標準搭載する」
//! （SameSite=Laxのみに依存しないCSRF対策の強化）を実装した。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::extract::Request;
use axum::http::{header, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

fn forbidden_response(detail: &str) -> Response {
    (
        StatusCode::FORBIDDEN,
        Json(json!({ "error": "origin_check_failed", "detail": detail })),
    )
        .into_response()
}

/// 状態変更メソッド（GET/HEAD/OPTIONS以外）について、`Origin`（無ければ`Referer`）
/// ヘッダーが許可リストのいずれかで始まるかを検証する。
/// SameSite Cookieだけに頼らない多層防御として、httpOnly Cookie方式（6章）と
/// 組み合わせて使うことを想定する。
pub fn origin_check_middleware(
    allowed_origins: Arc<Vec<String>>,
) -> impl Fn(Request, Next) -> BoxFuture<Response> + Clone {
    move |req: Request, next: Next| {
        let allowed = allowed_origins.clone();
        Box::pin(async move {
            let method = req.method().clone();
            if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
                return next.run(req).await;
            }

            let origin_or_referer = req
                .headers()
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok())
                .or_else(|| {
                    req.headers()
                        .get(header::REFERER)
                        .and_then(|v| v.to_str().ok())
                });

            match origin_or_referer {
                Some(value)
                    if allowed
                        .iter()
                        .any(|allowed_origin| value.starts_with(allowed_origin.as_str())) =>
                {
                    next.run(req).await
                }
                Some(_) => forbidden_response("Origin/Refererが許可リストに含まれていません"),
                None => forbidden_response("Origin/Refererヘッダーがありません"),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::post;
    use axum::Router;
    use tower::ServiceExt;

    #[tokio::test]
    async fn rejects_disallowed_origin_on_post() {
        let allowed = Arc::new(vec!["https://senn.example.com".to_string()]);
        let app: Router = Router::new()
            .route("/x", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn(origin_check_middleware(allowed)));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/x")
                    .header(header::ORIGIN, "https://evil.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn allows_matching_origin_on_post() {
        let allowed = Arc::new(vec!["https://senn.example.com".to_string()]);
        let app: Router = Router::new()
            .route("/x", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn(origin_check_middleware(allowed)));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/x")
                    .header(header::ORIGIN, "https://senn.example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
