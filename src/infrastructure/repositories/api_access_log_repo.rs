/// infrastructure/repositories/api_access_log_repo.rs — APIアクセスログ記録

use uuid::Uuid;

/// APIアクセスログを挿入
pub async fn insert_log(
    pool: &sqlx::PgPool,
    api_key_id: Uuid,
    action: &str,
    related_table: &str,
    related_id: &str,
    http_status: i16,
    ip_address: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO t_api_access_log (api_key_id, action, related_table, related_id, http_status, ip_address) \
         VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(api_key_id)
    .bind(action)
    .bind(related_table)
    .bind(related_id)
    .bind(http_status)
    .bind(ip_address)
    .execute(pool)
    .await?;
    Ok(())
}
