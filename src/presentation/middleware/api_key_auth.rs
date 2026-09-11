/// presentation/middleware/api_key_auth.rs — APIキー認証ミドルウェア

use axum::{
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use serde_json::json;
use sqlx::PgPool;
use sha2::{Sha256, Digest};
use uuid::Uuid;
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::errors::GovernorError;

/// APIキー認証情報（リクエストのExtensionsに注入）
#[derive(Debug, Clone)]
pub struct AuthApiKey {
    pub id: Uuid,
    pub party_type: String,
    pub party_id: String,
    pub scope: String,
}

/// api_key_id単位のレート制限用 KeyExtractor
#[derive(Clone)]
pub struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    type Key = Uuid;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        req.extensions()
            .get::<AuthApiKey>()
            .map(|auth| auth.id)
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

/// APIキー認証ミドルウェア
pub async fn api_key_auth(
    State(pool): State<PgPool>,
    mut req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    // Authorization: Bearer sk_live_... ヘッダを取得
    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let api_key_str = match auth_header {
        Some(key) => key,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid_api_key" })),
            ))
        }
    };

    // SHA256ハッシュ化
    let mut hasher = Sha256::new();
    hasher.update(&api_key_str);
    let api_key_hash = format!("{:x}", hasher.finalize());

    // t_company_api_key を検索
    let api_key = sqlx::query_as::<_, (Uuid, String, String, String)>(
        "SELECT id, party_type, party_id, scope FROM t_company_api_key \
         WHERE api_key_hash = $1 AND is_active = true AND revoked_at IS NULL"
    )
    .bind(&api_key_hash)
    .fetch_optional(&pool)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database_error" })),
        )
    })?;

    let (id, party_type, party_id, scope) = match api_key {
        Some(key) => key,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid_api_key" })),
            ))
        }
    };

    // AuthApiKey をExtensionsに注入
    let auth = AuthApiKey {
        id,
        party_type,
        party_id,
        scope,
    };
    req.extensions_mut().insert(auth);

    // last_used_at を非同期で更新（レスポンスをブロックしない）
    let pool_clone = pool.clone();
    let id_clone = id;
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE t_company_api_key SET last_used_at = NOW() WHERE id = $1")
            .bind(id_clone)
            .execute(&pool_clone)
            .await;
    });

    Ok(next.run(req).await)
}
