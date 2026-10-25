/// infrastructure/repositories/org_change_log_repo.rs — 組織・ロール・割り当ての変更履歴(h_org_role_change)

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgConnection, PgExecutor};

/// 管理操作を直列化するためのアドバイザリロックのキー(組織・ロール・割り当ての変更で共通)。
const ADMIN_CHANGE_LOCK_KEY: i64 = 173_005;

/// 履歴1件(変更者の表示名つき)。
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct ChangeLogRow {
    pub id: i64,
    pub changed_at: DateTime<Utc>,
    pub changed_by: Option<i64>,
    pub changed_by_name: Option<String>,
    pub target_kind: String,
    pub target_id: Option<i64>,
    pub action: String,
    pub before_json: Option<Value>,
    pub after_json: Option<Value>,
}

/// 組織・ロール・割り当ての管理操作を、トランザクションの終わりまで直列化する。
/// 検証(木・権限セット)とそれに続く更新の間に、別の管理操作が割り込むのを防ぐ。
pub async fn lock_admin_changes(conn: &mut PgConnection) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ADMIN_CHANGE_LOCK_KEY)
        .execute(conn)
        .await?;
    Ok(())
}

/// 履歴を1行追加する。更新と同じトランザクションで呼ぶ。
pub async fn insert(
    conn: &mut PgConnection,
    changed_by: i64,
    target_kind: &str,
    target_id: Option<i64>,
    action: &str,
    before: Option<Value>,
    after: Option<Value>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO h_org_role_change (changed_by, target_kind, target_id, action, before_json, after_json) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(changed_by)
    .bind(target_kind)
    .bind(target_id)
    .bind(action)
    .bind(before)
    .bind(after)
    .execute(conn)
    .await?;
    Ok(())
}

/// 新しい順に履歴を返す。
pub async fn list_recent<'e>(db: impl PgExecutor<'e>, limit: i64) -> Result<Vec<ChangeLogRow>> {
    let rows = sqlx::query_as::<_, ChangeLogRow>(
        "SELECT h.id, h.changed_at, h.changed_by, u.username AS changed_by_name, h.target_kind, h.target_id, \
                h.action, h.before_json, h.after_json \
         FROM h_org_role_change h LEFT JOIN s_user u ON u.id = h.changed_by \
         ORDER BY h.id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
