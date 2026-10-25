/// infrastructure/repositories/org_assignment_repo.rs — ユーザーの割り当て(s_user_org_role)

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgConnection, PgExecutor};

use crate::domain::authz::decision::AssignmentRow;

#[derive(sqlx::FromRow)]
struct AssignRow {
    org_unit_id: i64,
    role_id: i64,
}

/// 管理画面用の割り当て1件(ユーザー・組織・ロールの表示名つき)。
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AssignmentDetail {
    pub id: i64,
    pub user_id: i64,
    pub user_email: String,
    pub username: String,
    pub org_unit_id: i64,
    pub org_unit_name: String,
    pub role_id: i64,
    pub role_code: String,
    pub role_name: String,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<i64>,
}

/// 削除した割り当て(履歴に残す用)。
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct AssignmentRecord {
    pub id: i64,
    pub user_id: i64,
    pub org_unit_id: i64,
    pub role_id: i64,
}

/// ユーザーの割り当て一覧。有効な組織への割り当てだけを返す(無効な組織は権限として働かない)。
pub async fn list_for_user(pool: &sqlx::PgPool, user_id: i64) -> Result<Vec<AssignmentRow>> {
    let rows = sqlx::query_as::<_, AssignRow>(
        "SELECT a.org_unit_id, a.role_id FROM s_user_org_role a \
         JOIN m_org_unit o ON o.id = a.org_unit_id \
         WHERE a.user_id = $1 AND o.is_active = TRUE \
         ORDER BY a.id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| AssignmentRow { org_unit_id: r.org_unit_id, role_id: r.role_id }).collect())
}

/// 全割り当ての一覧(ユーザー・組織・ロールの表示名つき)。
pub async fn list_all<'e>(db: impl PgExecutor<'e>) -> Result<Vec<AssignmentDetail>> {
    let rows = sqlx::query_as::<_, AssignmentDetail>(
        "SELECT a.id, a.user_id, u.email AS user_email, u.username, a.org_unit_id, o.name AS org_unit_name, \
                a.role_id, r.code AS role_code, r.name AS role_name, a.created_at, a.created_by \
         FROM s_user_org_role a \
         JOIN s_user u ON u.id = a.user_id \
         JOIN m_org_unit o ON o.id = a.org_unit_id \
         JOIN m_role r ON r.id = a.role_id \
         ORDER BY u.username, a.id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// 割り当てを1件取得する(履歴用)。
pub async fn find<'e>(db: impl PgExecutor<'e>, id: i64) -> Result<Option<AssignmentRecord>> {
    let row = sqlx::query_as::<_, AssignmentRecord>(
        "SELECT id, user_id, org_unit_id, role_id FROM s_user_org_role WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// 割り当てを作る(検証は呼び出し側で済ませる)。同じ組み合わせが既にあれば一意制約違反。
pub async fn create(conn: &mut PgConnection, user_id: i64, org_unit_id: i64, role_id: i64, created_by: i64) -> Result<AssignmentRecord> {
    let row = sqlx::query_as::<_, AssignmentRecord>(
        "INSERT INTO s_user_org_role (user_id, org_unit_id, role_id, created_by) VALUES ($1, $2, $3, $4) \
         RETURNING id, user_id, org_unit_id, role_id",
    )
    .bind(user_id)
    .bind(org_unit_id)
    .bind(role_id)
    .bind(created_by)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// 割り当てを1件削除する。削除した行を返す(無ければ `None`)。
pub async fn delete(conn: &mut PgConnection, id: i64) -> Result<Option<AssignmentRecord>> {
    let row = sqlx::query_as::<_, AssignmentRecord>(
        "DELETE FROM s_user_org_role WHERE id = $1 RETURNING id, user_id, org_unit_id, role_id",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

/// ユーザーの割り当てをすべて削除する。削除した行を返す。
pub async fn delete_all_for_user(conn: &mut PgConnection, user_id: i64) -> Result<Vec<AssignmentRecord>> {
    let rows = sqlx::query_as::<_, AssignmentRecord>(
        "DELETE FROM s_user_org_role WHERE user_id = $1 RETURNING id, user_id, org_unit_id, role_id",
    )
    .bind(user_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// ユーザーの有効フラグ(割り当て作成の前提確認用)。ユーザーが無ければ `None`。
pub async fn user_is_active<'e>(db: impl PgExecutor<'e>, user_id: i64) -> Result<Option<bool>> {
    let v = sqlx::query_scalar::<_, bool>("SELECT is_active FROM s_user WHERE id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(v)
}
