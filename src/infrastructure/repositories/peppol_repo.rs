/// infrastructure/repositories/peppol_repo.rs — Peppol送受信ログCRUD

use sqlx::PgPool;

use crate::domain::models::peppol::PeppolTransmission;

#[allow(clippy::too_many_arguments)]
pub async fn insert_transmission(
    pool: &PgPool,
    direction: &str,
    document_type: &str,
    related_table: &str,
    related_id: &str,
    peppol_message_id: &str,
    participant_id: &str,
    status: &str,
    request_payload: Option<serde_json::Value>,
    response_payload: Option<serde_json::Value>,
    error_message: &str,
) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_peppol_transmission (
            direction, document_type, related_table, related_id,
            peppol_message_id, participant_id, status,
            request_payload, response_payload, error_message
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id
        "#,
    )
    .bind(direction)
    .bind(document_type)
    .bind(related_table)
    .bind(related_id)
    .bind(peppol_message_id)
    .bind(participant_id)
    .bind(status)
    .bind(request_payload)
    .bind(response_payload)
    .bind(error_message)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 未処理優先の一覧（FAILED/PENDING/UNMATCHEDのみ）
pub async fn list_actionable(pool: &PgPool) -> anyhow::Result<Vec<PeppolTransmission>> {
    let rows = sqlx::query_as::<_, PeppolTransmission>(
        "SELECT * FROM t_peppol_transmission WHERE status IN ('FAILED', 'PENDING', 'UNMATCHED') ORDER BY occurred_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 全件一覧
pub async fn list_all(pool: &PgPool) -> anyhow::Result<Vec<PeppolTransmission>> {
    let rows = sqlx::query_as::<_, PeppolTransmission>(
        "SELECT * FROM t_peppol_transmission ORDER BY occurred_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn mark_matched(pool: &PgPool, id: i64, related_table: &str, related_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE t_peppol_transmission SET status = 'MATCHED', related_table = $1, related_id = $2 WHERE id = $3",
    )
    .bind(related_table)
    .bind(related_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
