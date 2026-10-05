use axum::body::Body;
use axum::http::{response::Builder, StatusCode};
use axum::response::Response;

/// Builder からレスポンスを組み立てる。組み立て失敗は 500 にして握りつぶさずログ出力。
///
/// ステータス・ヘッダが定数の箇所では通常失敗しないが、パニックさせない（開発標準書 §3.3）。
pub fn build_response(builder: Builder, body: Body) -> Response {
    builder.body(body).unwrap_or_else(|e| {
        tracing::error!("Response builder failed: {}", e);
        // Response::new は失敗しない（Builder を経由しないためヘッダ検証が無い）
        let mut fallback = Response::new(Body::empty());
        *fallback.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        fallback
    })
}
