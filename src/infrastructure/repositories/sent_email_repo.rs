//! infrastructure/repositories/sent_email_repo.rs — 送信メールの記録(h_sent_email)の読み取り(DEMO-000196)
//!
//! 記録は email_template_repo::insert_sent_email_log が、Sophia から送るすべてのメールについて書く。
//! ここは「メール > 送信」画面の一覧・本文表示のための読み取りだけを持つ。

use anyhow::Result;
use sqlx::PgPool;

/// 一覧に出す最大件数(新しい順)
pub const LIST_LIMIT: i64 = 500;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SentEmailRow {
    pub id: i64,
    pub recipient: String,
    pub subject: String,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SentEmailDetail {
    pub id: i64,
    pub recipient: String,
    pub subject: String,
    pub body: String,
    pub sent_at: chrono::DateTime<chrono::Utc>,
}

/// 送信メールの一覧(新しい順、最大 LIST_LIMIT 件)
pub async fn list_recent(pool: &PgPool) -> Result<Vec<SentEmailRow>> {
    let rows = sqlx::query_as::<_, SentEmailRow>(
        "SELECT id, recipient, subject, sent_at FROM h_sent_email ORDER BY sent_at DESC, id DESC LIMIT $1",
    )
    .bind(LIST_LIMIT)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 送信メール1件(本文つき)
pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<SentEmailDetail>> {
    let row = sqlx::query_as::<_, SentEmailDetail>(
        "SELECT id, recipient, subject, body, sent_at FROM h_sent_email WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
