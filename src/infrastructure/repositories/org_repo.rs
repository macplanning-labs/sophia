/// infrastructure/repositories/org_repo.rs — 組織(m_org_unit)の読み出しと更新

use anyhow::Result;
use serde::Serialize;
use sqlx::{PgConnection, PgExecutor};

/// 組織1件。
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct OrgUnitRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub unit_kind: Option<String>,
    pub sort_order: i32,
    pub is_active: bool,
}

/// 全組織の一覧(表示順 → ID順)。
///
/// 無効な組織も含める。木を作るときに無効な組織が欠けると、その子が親を失って
/// `OrgTree::new` が失敗するため。
pub async fn list_all<'e>(db: impl PgExecutor<'e>) -> Result<Vec<OrgUnitRow>> {
    let rows = sqlx::query_as::<_, OrgUnitRow>(
        "SELECT id, parent_id, name, unit_kind, sort_order, is_active \
         FROM m_org_unit ORDER BY sort_order, id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// 組織を1件取得する。
pub async fn find<'e>(db: impl PgExecutor<'e>, id: i64) -> Result<Option<OrgUnitRow>> {
    let row = sqlx::query_as::<_, OrgUnitRow>("SELECT id, parent_id, name, unit_kind, sort_order, is_active FROM m_org_unit WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

/// 組織を作る。
pub async fn create(
    conn: &mut PgConnection,
    parent_id: Option<i64>,
    name: &str,
    unit_kind: Option<&str>,
    sort_order: i32,
) -> Result<OrgUnitRow> {
    let row = sqlx::query_as::<_, OrgUnitRow>("INSERT INTO m_org_unit (parent_id, name, unit_kind, sort_order) VALUES ($1, $2, $3, $4) RETURNING id, parent_id, name, unit_kind, sort_order, is_active")
    .bind(parent_id)
    .bind(name)
    .bind(unit_kind)
    .bind(sort_order)
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// 名前・種別・表示順を更新する。
pub async fn update(
    conn: &mut PgConnection,
    id: i64,
    name: &str,
    unit_kind: Option<&str>,
    sort_order: i32,
) -> Result<Option<OrgUnitRow>> {
    let row = sqlx::query_as::<_, OrgUnitRow>("UPDATE m_org_unit SET name = $2, unit_kind = $3, sort_order = $4, updated_at = NOW() \
         WHERE id = $1 RETURNING id, parent_id, name, unit_kind, sort_order, is_active")
    .bind(id)
    .bind(name)
    .bind(unit_kind)
    .bind(sort_order)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

/// 親を付け替える。
pub async fn move_to(conn: &mut PgConnection, id: i64, new_parent: i64) -> Result<Option<OrgUnitRow>> {
    let row = sqlx::query_as::<_, OrgUnitRow>("UPDATE m_org_unit SET parent_id = $2, updated_at = NOW() WHERE id = $1 RETURNING id, parent_id, name, unit_kind, sort_order, is_active")
    .bind(id)
    .bind(new_parent)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}

/// 有効/無効を切り替える。
pub async fn set_active(conn: &mut PgConnection, id: i64, is_active: bool) -> Result<Option<OrgUnitRow>> {
    let row = sqlx::query_as::<_, OrgUnitRow>("UPDATE m_org_unit SET is_active = $2, updated_at = NOW() WHERE id = $1 RETURNING id, parent_id, name, unit_kind, sort_order, is_active")
    .bind(id)
    .bind(is_active)
    .fetch_optional(conn)
    .await?;
    Ok(row)
}
