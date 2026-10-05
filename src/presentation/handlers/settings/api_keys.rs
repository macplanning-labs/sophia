/// presentation/handlers/settings/api_keys.rs — APIキー管理（Admin用）

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use rand::{rngs::OsRng, Rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::repositories::api_key_repo;

#[derive(Debug, Deserialize)]
pub struct ListApiKeysQuery {
    pub client_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateApiKeyRequest {
    pub client_id: i64,
    pub name: String,
    pub scope: String,
    pub environment: String,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub key_prefix: String,
    pub scope: String,
    pub name: String,
    pub is_active: bool,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GenerateApiKeyResponse {
    pub id: String,
    pub api_key: String,
    pub key_prefix: String,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorResponse {
    pub error: String,
    pub message: Option<String>,
}

/// GET /api/v1/settings/api-keys
pub async fn list_api_keys(
    State(pool): State<PgPool>,
    Query(params): Query<ListApiKeysQuery>,
) -> Result<Json<Vec<ApiKeyResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    let client_id = params.client_id
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("client_id is required".to_string()),
            }),
        ))?;

    let keys = api_key_repo::list_by_party(&pool, "CLIENT", &client_id.to_string()).await
        .map_err(|e| {
            tracing::error!("Failed to list API keys: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to fetch keys".to_string()),
                }),
            )
        })?;

    let responses = keys.into_iter().map(|k| ApiKeyResponse {
        id: k.id.to_string(),
        key_prefix: k.key_prefix,
        scope: k.scope,
        name: k.name,
        is_active: k.is_active,
        created_at: k.created_at.to_rfc3339(),
        revoked_at: k.revoked_at.map(|dt| dt.to_rfc3339()),
    }).collect();

    Ok(Json(responses))
}

/// POST /api/v1/settings/api-keys — 新規発行（生キーはレスポンス 1 回のみ）
pub async fn generate_api_key(
    State(pool): State<PgPool>,
    Json(payload): Json<GenerateApiKeyRequest>,
) -> Result<(StatusCode, Json<GenerateApiKeyResponse>), (StatusCode, Json<ApiErrorResponse>)> {
    if payload.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("name is required".to_string()),
            }),
        ));
    }

    if payload.scope != "READ" && payload.scope != "READ_WRITE" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("scope must be READ or READ_WRITE".to_string()),
            }),
        ));
    }

    let key_prefix = match payload.environment.as_str() {
        "live" => "sk_live_",
        "test" => "sk_test_",
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiErrorResponse {
                    error: "invalid_parameter".to_string(),
                    message: Some("environment must be live or test".to_string()),
                }),
            ));
        }
    };

    let mut rng = OsRng;
    let random_suffix: String = (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();
    let api_key = format!("{key_prefix}{random_suffix}");

    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    let api_key_hash = format!("{:x}", hasher.finalize());

    let id = api_key_repo::insert_key(
        &pool,
        "CLIENT",
        &payload.client_id.to_string(),
        key_prefix,
        &api_key_hash,
        &payload.scope,
        payload.name.trim(),
    )
    .await
    .map_err(|e| {
        tracing::error!("[APIキー/発行] 処理=insert_key 結果=失敗 | {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiErrorResponse {
                error: "database_error".to_string(),
                message: Some("Failed to generate key".to_string()),
            }),
        )
    })?;

    Ok((
        StatusCode::OK,
        Json(GenerateApiKeyResponse {
            id: id.to_string(),
            api_key,
            key_prefix: key_prefix.to_string(),
        }),
    ))
}

/// POST /api/v1/settings/api-keys/{id}/revoke
pub async fn revoke_api_key(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    let success = api_key_repo::revoke_key(&pool, id).await
        .map_err(|e| {
            tracing::error!("Failed to revoke API key: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to revoke key".to_string()),
                }),
            )
        })?;

    if !success {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiErrorResponse {
                error: "not_found".to_string(),
                message: Some("API key not found or already revoked".to_string()),
            }),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}
