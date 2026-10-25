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

    // 失敗の記録に使う。キー・ハッシュは出さない
    let ip = crate::presentation::auth_messages::client_ip(crate::presentation::auth_messages::peer_of(&req), req.headers());

    // t_company_api_key を検索（期限は取得後に判定する。期限切れと不明なキーをログで区別するため）
    let api_key = sqlx::query_as::<_, (Uuid, String, String, String, Option<chrono::DateTime<chrono::Utc>>)>(
        "SELECT id, party_type, party_id, scope, expires_at FROM t_company_api_key \
         WHERE api_key_hash = $1 AND is_active = true AND revoked_at IS NULL"
    )
    .bind(&api_key_hash)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        tracing::error!("[APIキー認証] 処理=キー検索 結果=失敗 影響=500で返却 | {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "database_error" })),
        )
    })?;

    let (id, party_type, party_id, scope) = match api_key {
        Some((id, party_type, party_id, scope, expires_at)) => {
            // 期限切れは、存在しないキーと同じ 401（応答で理由を教えない）。ログには理由を残す
            if crate::domain::services::api_key_policy::is_expired(expires_at, chrono::Utc::now()) {
                tracing::warn!(
                    "[APIキー認証] 処理=認証 結果=失敗(期限切れ) 影響=401で返却 | key_id={} ip={}",
                    id, ip
                );
                // 連続したら知らせる(ブロックはしない。DEMO-000181 項目6)
                crate::presentation::middleware::probe_log::record_api_key_failure(&ip);
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": "invalid_api_key" })),
                ));
            }
            (id, party_type, party_id, scope)
        }
        None => {
            tracing::warn!(
                "[APIキー認証] 処理=認証 結果=失敗(不明・無効・失効) 影響=401で返却 | ip={}",
                ip
            );
            // 連続したら知らせる(ブロックはしない。DEMO-000181 項目6)
            crate::presentation::middleware::probe_log::record_api_key_failure(&ip);
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid_api_key" })),
            ));
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
