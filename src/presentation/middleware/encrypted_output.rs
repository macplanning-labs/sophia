//! JSON 応答に暗号文(`enc1:`)が混ざったら止める(DEMO-000233 S2-G2-1)。
//!
//! 通常の画面・API は平文の JSON を返す。暗号文が出るのは、読み取りの暗号化経由になっていない経路が
//! あるときだけなので、そこで 500 にして外へ出さない。ログには経路(パス)と原因だけを記録し、値は出さない。
//! JSON 以外の応答(画面の HTML、ファイルのダウンロードなど)は検査しない。
//! 検査するのは Content-Type が JSON の応答だけ。帳票(PDF・CSV・HTML)に暗号文が出ないことは、
//! 帳票を作る箇所で `ensure_no_encrypted_payload` を呼んで確かめる(設計書 G2-2 の必須項目)。

use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::infrastructure::data_crypto::ensure_no_encrypted_payload;

/// 検査する応答の上限(これを超える JSON は検査できないので止める)
const MAX_INSPECT_BYTES: usize = 32 * 1024 * 1024;

/// Content-Type が JSON か。大文字小文字は問わず、`; charset=...` などのパラメータは除いて判定する。
/// `application/json`・`text/json`・`application/problem+json` のような `+json` の型も対象にする。
fn is_json_content_type(value: &str) -> bool {
    let media = value.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    media == "text/json" || media.ends_with("/json") || media.ends_with("+json")
}

pub async fn block_encrypted_json(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let res = next.run(req).await;

    let is_json = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(is_json_content_type);
    if !is_json {
        return res;
    }

    let (parts, body) = res.into_parts();
    let bytes = match to_bytes(body, MAX_INSPECT_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            // 検査できない応答は出さない(閉じる側に倒す)
            tracing::error!(path = %path, "JSON 応答を検査できない大きさ(32MB 超)または読めない本文のため、500 にしました");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let text = String::from_utf8_lossy(&bytes);
    if ensure_no_encrypted_payload(&text).is_err() {
        tracing::error!(path = %path, "JSON 応答に暗号化された値が混ざっていたため、500 にしました(値は記録しません)");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    axum::response::Response::from_parts(parts, Body::from(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        http::{HeaderValue, Method},
        middleware,
        routing::get,
        Json, Router,
    };
    use tower::ServiceExt;
    use tower_http::cors::{AllowOrigin, CorsLayer};

    /// 形の上で正しい暗号文(38 文字以上の base64)。実際の値ではない
    const SAMPLE: &str = "enc1:k1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    const LEAK_JSON: &str = "{\"account_number\":\"enc1:k1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}";

    fn app() -> Router {
        Router::new()
            .route("/plain", get(|| async { Json(serde_json::json!({"account_number": "1234567"})) }))
            .route("/leak", get(|| async { Json(serde_json::json!({"account_number": SAMPLE})) }))
            .route("/html", get(|| async { SAMPLE }))
            .layer(middleware::from_fn(block_encrypted_json))
    }

    async fn get_path(path: &str) -> Response {
        app().oneshot(axum::http::Request::get(path).body(Body::empty()).expect("request")).await.expect("応答")
    }

    /// 1 本の経路が、指定の Content-Type と本文を返す応答を作って通す
    async fn respond(method: Method, ct: &'static str, status: StatusCode, body: Vec<u8>) -> Response {
        let body = std::sync::Arc::new(body);
        let route = move || {
            let body = body.clone();
            async move { (status, [(header::CONTENT_TYPE, ct)], body.to_vec()) }
        };
        // get の経路は HEAD にも応答する(axum の既定)
        let app = Router::new()
            .route("/t", get(route))
            .layer(middleware::from_fn(block_encrypted_json));
        app.oneshot(axum::http::Request::builder().method(method).uri("/t").body(Body::empty()).expect("request"))
            .await
            .expect("応答")
    }

    async fn status_of(method: Method, ct: &'static str, status: StatusCode, body: &str) -> StatusCode {
        respond(method, ct, status, body.as_bytes().to_vec()).await.status()
    }

    #[tokio::test]
    async fn plain_json_passes_through() {
        let res = get_path("/plain").await;
        assert_eq!(res.status(), StatusCode::OK);
        let bytes = to_bytes(res.into_body(), 4096).await.expect("本文");
        assert!(String::from_utf8_lossy(&bytes).contains("1234567"));
    }

    #[tokio::test]
    async fn json_with_encrypted_value_is_stopped_without_the_value() {
        let res = get_path("/leak").await;
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(res.into_body(), 4096).await.expect("本文");
        assert!(!String::from_utf8_lossy(&bytes).contains("enc1:"), "暗号文を外へ出さない");
    }

    #[tokio::test]
    async fn non_json_responses_are_not_inspected() {
        let res = get_path("/html").await;
        assert_eq!(res.status(), StatusCode::OK, "JSON 以外は検査しない");
    }

    #[tokio::test]
    async fn json_content_types_are_recognised_with_parameters_and_case() {
        for ct in [
            "application/json",
            "application/json; charset=utf-8",
            "Application/JSON",
            "application/problem+json",
            "application/problem+json; charset=utf-8",
            "text/json",
        ] {
            assert_eq!(status_of(Method::GET, ct, StatusCode::OK, LEAK_JSON).await, StatusCode::INTERNAL_SERVER_ERROR, "{ct} は検査する");
        }
    }

    #[tokio::test]
    async fn other_content_types_are_not_inspected() {
        for ct in ["text/html; charset=utf-8", "text/plain", "application/octet-stream", "application/jsonl-ish"] {
            assert_eq!(status_of(Method::GET, ct, StatusCode::OK, LEAK_JSON).await, StatusCode::OK, "{ct} は検査しない");
        }
    }

    #[tokio::test]
    async fn head_responses_are_handled() {
        // 本文の無い HEAD は通す
        assert_eq!(status_of(Method::HEAD, "application/json", StatusCode::OK, "").await, StatusCode::OK);
        // HEAD でも本文に暗号文があれば止める(閉じる側)
        assert_eq!(status_of(Method::HEAD, "application/json", StatusCode::OK, LEAK_JSON).await, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn no_content_and_not_modified_pass_through() {
        assert_eq!(status_of(Method::GET, "application/json", StatusCode::NO_CONTENT, "").await, StatusCode::NO_CONTENT);
        assert_eq!(status_of(Method::GET, "application/json", StatusCode::NOT_MODIFIED, "").await, StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn empty_json_body_passes() {
        assert_eq!(status_of(Method::GET, "application/json", StatusCode::OK, "").await, StatusCode::OK);
    }

    #[tokio::test]
    async fn oversized_json_is_stopped_even_without_a_leak() {
        // 32MB を超える JSON は検査できないので 500(平文でも止める)
        let big = vec![b'a'; MAX_INSPECT_BYTES + 1];
        let res = respond(Method::GET, "application/json", StatusCode::OK, big).await;
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn json_at_the_size_limit_is_inspected_and_passes() {
        let ok = vec![b' '; MAX_INSPECT_BYTES];
        let res = respond(Method::GET, "application/json", StatusCode::OK, ok).await;
        assert_eq!(res.status(), StatusCode::OK, "上限ちょうどは検査して通す");
    }

    #[tokio::test]
    async fn enc1_text_in_a_memo_passes() {
        let memo = "{\"memo\":\"enc1: は暗号化の接頭辞です\"}";
        assert_eq!(status_of(Method::GET, "application/json", StatusCode::OK, memo).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn blocked_500_keeps_cors_headers_for_the_desktop_app() {
        // desktop_cors より内側に置いたときの並び(routes.rs と同じ)。500 にも CORS ヘッダーが付くこと
        let app = Router::new()
            .route("/leak", get(|| async { ([(header::CONTENT_TYPE, "application/json")], LEAK_JSON) }))
            .layer(middleware::from_fn(block_encrypted_json))
            .layer(
                CorsLayer::new()
                    .allow_origin(AllowOrigin::list([HeaderValue::from_static("tauri://localhost")]))
                    .allow_methods([Method::GET])
                    .allow_headers([header::CONTENT_TYPE]),
            );
        let req = axum::http::Request::get("/leak")
            .header(header::ORIGIN, "tauri://localhost")
            .body(Body::empty())
            .expect("request");
        let res = app.oneshot(req).await.expect("応答");
        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            res.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN).and_then(|v| v.to_str().ok()),
            Some("tauri://localhost"),
            "500 でも CORS の許可を付ける"
        );
    }

    #[test]
    fn json_content_type_rule() {
        assert!(is_json_content_type("application/json; charset=utf-8"));
        assert!(is_json_content_type("  Application/JSON  "));
        assert!(is_json_content_type("application/problem+json"));
        assert!(is_json_content_type("text/json"));
        assert!(!is_json_content_type("text/html"));
        assert!(!is_json_content_type("application/jsonl"));
        assert!(!is_json_content_type("application/x-json-foo"));
    }
}
