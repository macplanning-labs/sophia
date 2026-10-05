/// 開発・社内向け Swagger UI ハンドラ（ENABLE_SWAGGER_UI でゲート）
///
/// デフォルト OFF。本番での無防備公開を禁止する。
/// UI が配信する契約は取引先 `/v1`（`build_v1_api`）。社内契約は
/// `docs/openapi/sophia_internal_v1.json` / `export_openapi_internal` を参照。

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

/// GET /api/v1/docs/openapi.json — 取引先 /v1 OpenAPI スキーマ JSON
pub async fn openapi_json() -> impl IntoResponse {
    (StatusCode::OK, Json(crate::openapi::build_v1_api()))
}

/// GET /api/v1/docs — Swagger UI（簡易 HTML・CDN。ゲート ON 時のみ）
pub async fn swagger_ui() -> impl IntoResponse {
    let html = r#"<!DOCTYPE html>
<html lang="ja">
<head>
    <title>Sophia OpenAPI — 取引先 /v1</title>
    <meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.11.0/swagger-ui.min.css">
    <style>
        html { box-sizing: border-box; overflow-y: scroll; }
        *, *:before, *:after { box-sizing: inherit; }
        body { margin: 0; padding: 0; }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.11.0/swagger-ui-bundle.min.js"></script>
    <script src="https://cdnjs.cloudflare.com/ajax/libs/swagger-ui/5.11.0/swagger-ui-standalone-preset.min.js"></script>
    <script>
        SwaggerUIBundle({
            url: "/api/v1/docs/openapi.json",
            dom_id: '#swagger-ui',
            presets: [
                SwaggerUIBundle.presets.apis,
                SwaggerUIStandalonePreset
            ],
            layout: "StandaloneLayout",
            deepLinking: true
        });
    </script>
</body>
</html>"#;
    (StatusCode::OK, [("content-type", "text/html; charset=utf-8")], html)
}
