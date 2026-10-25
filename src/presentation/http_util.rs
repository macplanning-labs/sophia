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

/// 権限がないときの応答(403)。ハンドラ自身の権限判定で使う。他人のデータの有無は伝えない。
pub fn forbidden() -> Response {
    use axum::response::IntoResponse;
    (StatusCode::FORBIDDEN, axum::Json(serde_json::json!({ "error": "この操作の権限がありません" }))).into_response()
}
