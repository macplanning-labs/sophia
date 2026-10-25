/// infrastructure/repositories/role_repo.rs — ロール(m_role)と、ロールの権限(m_role_permission)

use anyhow::Result;
use serde::Serialize;
use sqlx::{PgConnection, PgExecutor};

use crate::domain::authz::decision::RolePermissionRow;

#[derive(sqlx::FromRow)]
struct PermRow {
    role_id: i64,
    permission_code: String,
    scope: String,
}

impl From<PermRow> for RolePermissionRow {
    fn from(r: PermRow) -> Self {
        RolePermissionRow { role_id: r.role_id, permission_code: r.permission_code, scope: r.scope }
    }
}

/// ロール1件。
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct RoleRow {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub is_system: bool,
}

/// 一覧用のロール(割り当て数つき)。
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct RoleListRow {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub is_system: bool,
    pub assignment_count: i64,
}

/// 指定したロールが持つ権限の一覧。
pub async fn list_permissions_for_roles<'e>(db: impl PgExecutor<'e>, role_ids: &[i64]) -> Result<Vec<RolePermissionRow>> {
    let rows = sqlx::query_as::<_, PermRow>(
        "SELECT role_id, permission_code, scope FROM m_role_permission \
         WHERE role_id = ANY($1) ORDER BY role_id, permission_code",
    )
    .bind(role_ids)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// ロールの一覧(割り当て数つき。標準ロール → コード順)。
pub async fn list_roles<'e>(db: impl PgExecutor<'e>) -> Result<Vec<RoleListRow>> {
    let rows = sqlx::query_as::<_, RoleListRow>(
        "SELECT r.id, r.code, r.name, r.is_system, \
                (SELECT COUNT(*) FROM s_user_org_role a WHERE a.role_id = r.id) AS assignment_count \
         FROM m_role r ORDER BY r.is_system DESC, r.code",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// ロールを1件取得する。
pub async fn find<'e>(db: impl PgExecutor<'e>, id: i64) -> Result<Option<RoleRow>> {
    let row = sqlx::query_as::<_, RoleRow>("SELECT id, code, name, is_system FROM m_role WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

/// ロールを作る(権限は空。標準ロールではない)。
pub async fn create_role(conn: &mut PgConnection, code: &str, name: &str) -> Result<RoleRow> {
    let row = sqlx::query_as::<_, RoleRow>(
        "INSERT INTO m_role (code, name) VALUES ($1, $2) RETURNING id, code, name, is_system",
    )
    .bind(code)
    .bind(name)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// ロールの表示名を変える(コードは変えない)。
pub async fn rename_role(conn: &mut PgConnection, id: i64, name: &str) -> Result<Option<RoleRow>> {
    let row = sqlx::query_as::<_, RoleRow>(
        "UPDATE m_role SET name = $2, updated_at = NOW() WHERE id = $1 RETURNING id, code, name, is_system",
    )
    .bind(id)
    .bind(name)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

/// ロールを削除する。標準ロール、または割り当てが残るロールは削除しない(戻り値 false)。
pub async fn delete_role(conn: &mut PgConnection, id: i64) -> Result<bool> {
    let r = sqlx::query(
        "DELETE FROM m_role WHERE id = $1 AND NOT is_system \
         AND NOT EXISTS (SELECT 1 FROM s_user_org_role a WHERE a.role_id = m_role.id)",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// ロールの権限セットを丸ごと置き換える(検証は呼び出し側で済ませる)。
pub async fn replace_permissions(conn: &mut PgConnection, role_id: i64, rows: &[RolePermissionRow]) -> Result<()> {
    sqlx::query("DELETE FROM m_role_permission WHERE role_id = $1")
        .bind(role_id)
        .execute(&mut *conn)
        .await?;
    for r in rows {
        sqlx::query("INSERT INTO m_role_permission (role_id, permission_code, scope) VALUES ($1, $2, $3)")
            .bind(role_id)
            .bind(&r.permission_code)
            .bind(&r.scope)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// ロールが今割り当てられている組織ID(重複なし)。
pub async fn assigned_org_ids<'e>(db: impl PgExecutor<'e>, role_id: i64) -> Result<Vec<i64>> {
    let ids = sqlx::query_scalar::<_, i64>(
        "SELECT DISTINCT org_unit_id FROM s_user_org_role WHERE role_id = $1 ORDER BY org_unit_id",
    )
    .bind(role_id)
    .fetch_all(db)
    .await?;
    Ok(ids)
}
