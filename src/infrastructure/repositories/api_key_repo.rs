/// infrastructure/repositories/api_key_repo.rs — APIキー管理

use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub party_type: String,
    pub party_id: String,
    pub key_prefix: String,
    pub scope: String,
    pub name: String,
    pub is_active: bool,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// パーティ（クライアント）に属する全APIキーを取得
pub async fn list_by_party(
    pool: &sqlx::PgPool,
    party_type: &str,
    party_id: &str,
) -> anyhow::Result<Vec<ApiKeyRow>> {
    let rows = sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, party_type, party_id, key_prefix, scope, name, is_active, last_used_at, created_at, revoked_at \
         FROM t_company_api_key WHERE party_type = $1 AND party_id = $2 ORDER BY created_at DESC"
    )
    .bind(party_type)
    .bind(party_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 新しいAPIキーを挿入
pub async fn insert_key(
    pool: &sqlx::PgPool,
    party_type: &str,
    party_id: &str,
    key_prefix: &str,
    api_key_hash: &str,
    scope: &str,
    name: &str,
) -> anyhow::Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO t_company_api_key (id, party_type, party_id, key_prefix, api_key_hash, scope, name, is_active) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, true)"
    )
    .bind(&id)
    .bind(party_type)
    .bind(party_id)
    .bind(key_prefix)
    .bind(api_key_hash)
    .bind(scope)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(id)
}

/// APIキーを失効させる
pub async fn revoke_key(pool: &sqlx::PgPool, id: Uuid) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE t_company_api_key SET is_active = false, revoked_at = NOW(), updated_at = NOW() \
         WHERE id = $1 AND is_active = true"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// アクティブなAPIキーメタデータを取得（SHA256ハッシュ検証用）
pub async fn find_active_key_meta(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> anyhow::Result<Option<ApiKeyRow>> {
    let row = sqlx::query_as::<_, ApiKeyRow>(
        "SELECT id, party_type, party_id, key_prefix, scope, name, is_active, last_used_at, created_at, revoked_at \
         FROM t_company_api_key WHERE id = $1 AND is_active = true"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
