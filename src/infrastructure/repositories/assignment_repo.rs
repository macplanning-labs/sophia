//! infrastructure/repositories/assignment_repo.rs — アサイン作成と利益試算のDBアクセス(UI刷新 2-6・2-7)

use anyhow::Result;
use sqlx::PgPool;

use crate::infrastructure::repositories::project_repo::{
    insert_client_contract, insert_partner_contract, resolve_engineer, EngineerRef,
    WizardClientContractInput, WizardPartnerContractInput,
};

/// 利益試算に必要な、案件側の情報
pub struct ProjectProfitContext {
    /// 案件の商流("DIRECT" / "SUBCONTRACT")。未設定なら None
    pub commercial_flow: Option<String>,
    /// 有効な受注契約の月額(工数×単価)の合計
    pub revenue: i64,
    /// 有効な発注契約の月額(工数×単価)の合計
    pub cost: i64,
}

/// 案件が無ければ None
pub async fn project_profit_context(pool: &PgPool, project_id: &str) -> Result<Option<ProjectProfitContext>> {
    let row: Option<(Option<String>,)> = sqlx::query_as("SELECT commercial_flow FROM m_project WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(pool)
        .await?;
    let Some((commercial_flow,)) = row else { return Ok(None) };

    let revenue: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ROUND(base_rate * effort)), 0)::BIGINT FROM m_client_contract
         WHERE project_id = $1 AND is_active = true AND end_date >= CURRENT_DATE",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    let cost: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(ROUND(base_rate * effort)), 0)::BIGINT FROM m_partner_contract
         WHERE project_id = $1 AND is_active = true AND end_date >= CURRENT_DATE",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(Some(ProjectProfitContext { commercial_flow, revenue, cost }))
}

/// 利益の目安(直受け%, 下請け%)。自社情報が無ければ既定の (20, 8)
pub async fn margin_targets(pool: &PgPool) -> Result<(i32, i32)> {
    let row: Option<(i32, i32)> =
        sqlx::query_as("SELECT target_margin_direct, target_margin_subcontract FROM s_company_info LIMIT 1")
            .fetch_optional(pool)
            .await?;
    Ok(row.unwrap_or((20, 8)))
}

/// 同じ要員の、直近の受注契約の月額(工数×単価)。なければ None
pub async fn previous_monthly_price(pool: &PgPool, engineer_id: i64) -> Result<Option<i32>> {
    let v: Option<i32> = sqlx::query_scalar(
        "SELECT ROUND(base_rate * effort)::INT FROM m_client_contract
         WHERE engineer_id = $1 ORDER BY start_date DESC, id DESC LIMIT 1",
    )
    .bind(engineer_id)
    .fetch_optional(pool)
    .await?;
    Ok(v)
}

/// 要員の区分("EMPLOYEE" / "PARTNER")。要員がいなければ None
pub async fn engineer_affiliation(pool: &PgPool, engineer_id: i64) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT affiliation_type FROM m_engineer WHERE id = $1")
        .bind(engineer_id)
        .fetch_optional(pool)
        .await?)
}

#[derive(Debug)]
pub enum AssignmentError {
    /// 案件・パートナーなどが見つからない(404)
    NotFound(String),
    /// 同じ要員・同じ開始日の受注契約が既にある(409)
    Duplicate(String),
    Other(anyhow::Error),
}

impl From<anyhow::Error> for AssignmentError {
    fn from(e: anyhow::Error) -> Self {
        Self::Other(e)
    }
}

impl From<sqlx::Error> for AssignmentError {
    fn from(e: sqlx::Error) -> Self {
        Self::Other(e.into())
    }
}

#[derive(Debug)]
pub struct CreatedAssignment {
    pub engineer_id: i64,
    pub client_contract_id: i64,
    pub partner_contract_id: Option<i64>,
}

/// アサインを作る。受注契約は常に、発注契約はパートナー要員のときだけ。1つのトランザクション(途中で失敗したら全部取り消す)
pub async fn create_assignment(
    pool: &PgPool,
    project_id: &str,
    engineer: &EngineerRef,
    client_contract: &WizardClientContractInput,
    partner_contract: Option<&WizardPartnerContractInput>,
    // 作業場所の名前(m_partner_contract.work_location に名前で保存する。マスタには追加しない)
    work_location_name: &str,
) -> std::result::Result<CreatedAssignment, AssignmentError> {
    let mut tx = pool.begin().await?;

    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM m_project WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(&mut *tx)
        .await?;
    if exists.is_none() {
        return Err(AssignmentError::NotFound(format!("案件 {project_id} が見つかりません")));
    }
    if let Some(pc) = partner_contract {
        let p: Option<i32> = sqlx::query_scalar("SELECT 1 FROM m_partner WHERE partner_id = $1")
            .bind(&pc.partner_id)
            .fetch_optional(&mut *tx)
            .await?;
        if p.is_none() {
            return Err(AssignmentError::NotFound(format!("提案元パートナー {} が見つかりません", pc.partner_id)));
        }
    }

    let engineer_id = resolve_engineer(&mut tx, engineer).await?;

    let client_contract_id = match insert_client_contract(&mut tx, project_id, engineer_id, client_contract).await {
        Ok(id) => id,
        Err(sqlx::Error::Database(db)) if db.code().as_deref() == Some("23505") => {
            return Err(AssignmentError::Duplicate(
                "同じ要員・同じ開始日の受注契約が、この案件に既にあります".to_string(),
            ));
        }
        Err(e) => return Err(e.into()),
    };

    let partner_contract_id = match partner_contract {
        Some(pc) => Some(
            insert_partner_contract(&mut tx, project_id, engineer_id, work_location_name, pc).await?,
        ),
        None => None,
    };

    crate::infrastructure::db_tx::commit_checked(tx).await?;
    Ok(CreatedAssignment { engineer_id, client_contract_id, partner_contract_id })
}

/// アサイン編成画面の案件カード用: 有効な案件(テスト案件を除く)ごとの、月額の売上・原価・アサイン数
#[derive(Debug, sqlx::FromRow)]
pub struct ProjectOverviewRow {
    pub project_id: String,
    pub project_name: String,
    pub client_id: i64,
    pub client_name: String,
    pub commercial_flow: Option<String>,
    /// 有効な受注契約の月額(工数×単価)の合計
    pub revenue: i64,
    /// 有効な発注契約の月額(工数×単価)の合計
    pub cost: i64,
    /// 有効な受注契約の数(=アサイン中の要員数)
    pub assignment_count: i64,
    /// うち、自社社員(原価が分からない=粗利に原価が含まれない)の数
    pub internal_count: i64,
}

pub async fn project_overview(pool: &PgPool) -> Result<Vec<ProjectOverviewRow>> {
    let rows = sqlx::query_as::<_, ProjectOverviewRow>(
        r#"
        SELECT p.project_id,
               p.name AS project_name,
               c.id AS client_id,
               c.name AS client_name,
               p.commercial_flow,
               COALESCE(cc.revenue, 0)::BIGINT AS revenue,
               COALESCE(pc.cost, 0)::BIGINT AS cost,
               COALESCE(cc.cnt, 0)::BIGINT AS assignment_count,
               COALESCE(cc.internal_cnt, 0)::BIGINT AS internal_count
        FROM m_project p
        JOIN m_client c ON c.id = p.client_id
        LEFT JOIN (
            SELECT mcc.project_id,
                   SUM(ROUND(mcc.base_rate * mcc.effort)) AS revenue,
                   COUNT(*) AS cnt,
                   COUNT(*) FILTER (WHERE e.affiliation_type = 'EMPLOYEE') AS internal_cnt
              FROM m_client_contract mcc
              JOIN m_engineer e ON e.id = mcc.engineer_id
             WHERE mcc.is_active = true AND mcc.end_date >= CURRENT_DATE
             GROUP BY mcc.project_id
        ) cc ON cc.project_id = p.project_id
        LEFT JOIN (
            SELECT mpc.project_id, SUM(ROUND(mpc.base_rate * mpc.effort)) AS cost
              FROM m_partner_contract mpc
             WHERE mpc.is_active = true AND mpc.end_date >= CURRENT_DATE
             GROUP BY mpc.project_id
        ) pc ON pc.project_id = p.project_id
        WHERE p.is_active = true AND p.is_test = false
        ORDER BY c.name, p.name
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

