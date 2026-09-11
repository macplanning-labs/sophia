/// infrastructure/repositories/mail_repo.rs — メール取込ログ(t_mail_scan_log) CRUD
///
/// ダッシュボードのメールチェック機能・EDI取込のトリガーで使用する。

use anyhow::Result;
use sqlx::PgPool;

/// メール取込ログ1件（ダッシュボード表示用）
#[derive(Clone, sqlx::FromRow, serde::Serialize)]
pub struct MailRow {
    pub id: i64,
    pub sender_name: String,
    pub sender_email: String,
    pub subject: String,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub classification: String,
    pub matched_entity_name: String,
    pub is_reflected: bool,
    pub note: String,
    pub body_text: String,
    pub attachments: String,
}

/// 未反映メール一覧（新しい順、最大50件）
pub async fn list_unreflected_mails(pool: &PgPool) -> Result<Vec<MailRow>> {
    let rows = sqlx::query_as::<_, MailRow>(
        r#"
        SELECT id, sender_name, sender_email, subject, received_at,
               classification, matched_entity_name, is_reflected, note,
               body_text, attachments
        FROM t_mail_scan_log
        WHERE is_reflected = FALSE
        ORDER BY received_at DESC
        LIMIT 50
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 直近90日で反映済みのメール件数
pub async fn count_confirmed_mails_recent(pool: &PgPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_mail_scan_log WHERE is_reflected = TRUE AND received_at > NOW() - INTERVAL '90 days'"
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// 全メールボックス中で最も新しい同期チェックポイント（ダッシュボード表示用）
pub async fn latest_sync_checkpoint(pool: &PgPool) -> Result<Option<chrono::DateTime<chrono::Utc>>> {
    let last_processed_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT MAX(last_processed_at) FROM s_mail_sync_checkpoint"
    )
    .fetch_one(pool)
    .await?;
    Ok(last_processed_at)
}

/// メールの反映済みフラグを更新する（更新件数を返す。0件なら対象が存在しない）
pub async fn set_mail_reflected(pool: &PgPool, id: i64, reflected: bool) -> Result<u64> {
    let result = sqlx::query("UPDATE t_mail_scan_log SET is_reflected = $1, updated_at = NOW() WHERE id = $2")
        .bind(reflected)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// GASからのWebhookで受信したメールを記録する（重複はgmail_message_idで無視）
#[allow(clippy::too_many_arguments)]
pub async fn insert_webhook_mail(
    pool: &PgPool,
    gmail_message_id: &str,
    sender_email: &str,
    sender_name: &str,
    subject: &str,
    received_at: chrono::DateTime<chrono::Utc>,
    classification: &str,
    matched_entity_name: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO t_mail_scan_log (gmail_message_id, sender_email, sender_name, subject, received_at, classification, matched_entity_name)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (gmail_message_id) DO NOTHING"#
    )
    .bind(gmail_message_id)
    .bind(sender_email)
    .bind(sender_name)
    .bind(subject)
    .bind(received_at)
    .bind(classification)
    .bind(matched_entity_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// メール本文を取得する（EDI取込のURL抽出用）
pub async fn find_mail_body(pool: &PgPool, mail_id: i64) -> Result<Option<String>> {
    let body: Option<String> = sqlx::query_scalar(
        "SELECT body_text FROM t_mail_scan_log WHERE id = $1"
    )
    .bind(mail_id)
    .fetch_optional(pool)
    .await?;
    Ok(body)
}
