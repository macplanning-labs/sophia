/// infrastructure/repositories/project_repo.rs — 案件(m_project)関連クエリ

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Postgres, Transaction};

/// 案件の (project_id, name) 一覧（フィルタ用プルダウン等）。有効な案件のみ。
pub async fn list_id_name_options(pool: &PgPool) -> Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT project_id, name FROM m_project WHERE is_active = true ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// クライアントの (id, name) 一覧（案件作成ウィザードのクライアント選択用）。
pub async fn list_client_id_name_options(pool: &PgPool) -> Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM m_client ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ══════════════════════════════════════════════════════════
// 案件一覧・詳細（/projects 専用ページ用）
// ══════════════════════════════════════════════════════════

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ProjectListRow {
    pub project_id: String,
    pub name: String,
    pub client_id: i64,
    pub client_name: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

pub async fn list_projects(pool: &PgPool) -> Result<Vec<ProjectListRow>> {
    let rows = sqlx::query_as::<_, ProjectListRow>(
        r#"
        SELECT p.project_id, p.name, p.client_id, c.name AS client_name, p.is_active, p.created_at
        FROM m_project p
        JOIN m_client c ON p.client_id = c.id
        ORDER BY p.created_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ProjectDetailRow {
    pub project_id: String,
    pub client_id: i64,
    pub client_name: String,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub edi_project_alias: String,
    pub report_deadline_type: String,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    pub report_request_day: Option<i32>,
    pub created_at: DateTime<Utc>,
}

pub async fn find_project_detail(pool: &PgPool, project_id: &str) -> Result<Option<ProjectDetailRow>> {
    let row = sqlx::query_as::<_, ProjectDetailRow>(
        r#"
        SELECT p.project_id, p.client_id, c.name AS client_name, p.name, p.description, p.is_active,
               COALESCE(p.edi_project_alias, '') AS edi_project_alias,
               p.report_deadline_type, p.report_deadline_value, p.report_deadline_holiday_rule,
               p.report_request_day, p.created_at
        FROM m_project p
        JOIN m_client c ON p.client_id = c.id
        WHERE p.project_id = $1
        "#
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ProjectClientContractSummary {
    pub id: i64,
    pub engineer_name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub is_active: bool,
}

pub async fn list_client_contract_summaries(pool: &PgPool, project_id: &str) -> Result<Vec<ProjectClientContractSummary>> {
    let rows = sqlx::query_as::<_, ProjectClientContractSummary>(
        r#"
        SELECT cc.id, e.name AS engineer_name, cc.start_date, cc.end_date, cc.is_active
        FROM m_client_contract cc
        JOIN m_engineer e ON cc.engineer_id = e.id
        WHERE cc.project_id = $1
        ORDER BY cc.start_date DESC
        "#
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ProjectPartnerContractSummary {
    pub id: i64,
    pub partner_name: String,
    pub engineer_name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub is_active: bool,
}

pub async fn list_partner_contract_summaries(pool: &PgPool, project_id: &str) -> Result<Vec<ProjectPartnerContractSummary>> {
    let rows = sqlx::query_as::<_, ProjectPartnerContractSummary>(
        r#"
        SELECT pc.id, pa.name AS partner_name, COALESCE(e.name, '') AS engineer_name,
               pc.start_date, pc.end_date, pc.is_active
        FROM m_partner_contract pc
        JOIN m_partner pa ON pc.partner_id = pa.partner_id
        LEFT JOIN m_engineer e ON pc.engineer_id = e.id
        WHERE pc.project_id = $1
        ORDER BY pc.start_date DESC
        "#
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ══════════════════════════════════════════════════════════
// 案件情報の更新(単発。作成ウィザードとは別。詳細ページの編集モーダル用)
// ══════════════════════════════════════════════════════════

pub struct ProjectUpdateInput {
    pub client_id: i64,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub edi_project_alias: String,
    pub report_deadline_type: String,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    pub report_request_day: Option<i32>,
}

/// 案件情報を更新する（m_project に updated_at カラムは無いため更新しない）。更新件数を返す。
pub async fn update_project(pool: &PgPool, project_id: &str, input: &ProjectUpdateInput) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE m_project SET
            client_id = $1, name = $2, description = $3, is_active = $4,
            edi_project_alias = $5,
            report_deadline_type = $6, report_deadline_value = $7,
            report_deadline_holiday_rule = $8, report_request_day = $9
        WHERE project_id = $10
        "#
    )
    .bind(input.client_id)
    .bind(&input.name)
    .bind(&input.description)
    .bind(input.is_active)
    .bind(&input.edi_project_alias)
    .bind(&input.report_deadline_type)
    .bind(input.report_deadline_value)
    .bind(&input.report_deadline_holiday_rule)
    .bind(input.report_request_day)
    .bind(project_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 案件を削除する（更新件数を返す）。契約等が紐づく場合は FK で失敗する。
pub async fn delete_project(pool: &PgPool, project_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM m_project WHERE project_id = $1")
        .bind(project_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ══════════════════════════════════════════════════════════
// 案件作成ウィザード — 1トランザクションで案件+クライアント契約(任意)+パートナー契約(0〜複数)を作成
// ══════════════════════════════════════════════════════════

/// 技術者のselect-or-create指定。JSON側は
/// `{"kind":"existing","id":123}` または
/// `{"kind":"new","name":"...","name_kana":null,"affiliation_type":"EMPLOYEE","partner_id":null,"email":null}`。
/// クライアント契約ステップ・パートナー契約ステップの両方で使う共通型
/// (2026-07-31、ユーザー確認: クライアント契約側のエンジニア選択にもselect-or-createが必要)。
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EngineerRef {
    Existing { id: i64 },
    New {
        name: String,
        name_kana: Option<String>,
        /// "EMPLOYEE" | "PARTNER"。domain::models::engineer::AffiliationType enum(値"INTERNAL")は
        /// 実データと不整合な死んだコードのため使わず、文字列をそのまま受け取る。
        affiliation_type: String,
        partner_id: Option<String>,
        email: Option<String>,
    },
}

/// 作業場所(m_work_location)のselect-or-create指定。
/// m_partner_contract.workplace_id(FK→m_workplace)は現行UIのどこからも使われていない
/// 死んだ配線のため対象にしない — m_work_locationが実際に運用されているテーブル。
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkLocationRef {
    Existing { id: i64 },
    New { name: String },
}

/// トランザクション内でengineerを解決する。Newなら m_engineer にINSERTしてidを返し、
/// Existingならそのまま返す。クライアント契約・パートナー契約の両ステップから呼ばれる共通関数。
async fn resolve_engineer(tx: &mut Transaction<'_, Postgres>, r: &EngineerRef) -> Result<i64> {
    match r {
        EngineerRef::Existing { id } => Ok(*id),
        EngineerRef::New { name, name_kana, affiliation_type, partner_id, email } => {
            // partner_id は自社社員(EMPLOYEE)の場合フロント側から空文字で届くことがあるため、
            // NULL可能なFKカラムに正しくNULLを入れられるよう空文字をNoneとして扱う。
            let partner_id = partner_id.as_deref().filter(|s| !s.is_empty());
            let id: i64 = sqlx::query_scalar(
                r#"
                INSERT INTO m_engineer (name, name_kana, affiliation_type, partner_id, email, is_active)
                VALUES ($1, $2, $3, $4, $5, true)
                RETURNING id
                "#
            )
            .bind(name)
            .bind(name_kana.as_deref().unwrap_or(""))
            .bind(affiliation_type)
            .bind(partner_id)
            .bind(email.as_deref().unwrap_or(""))
            .fetch_one(&mut **tx)
            .await
            .context("技術者の新規作成に失敗しました")?;
            Ok(id)
        }
    }
}

/// トランザクション内で作業場所(m_work_location)を解決する。Newならinsertしてidを返す。
/// m_partner_contract.work_location カラムはVARCHAR(名前を直接保存)のため、呼び出し側は
/// 返り値のidではなく解決済みの名前をINSERT文にそのまま使う。
async fn resolve_work_location(tx: &mut Transaction<'_, Postgres>, r: &WorkLocationRef) -> Result<String> {
    match r {
        WorkLocationRef::Existing { id } => {
            let name: String = sqlx::query_scalar("SELECT name FROM m_work_location WHERE id = $1")
                .bind(id)
                .fetch_one(&mut **tx)
                .await
                .context("作業場所が見つかりません")?;
            Ok(name)
        }
        WorkLocationRef::New { name } => {
            sqlx::query("INSERT INTO m_work_location (name) VALUES ($1)")
                .bind(name)
                .execute(&mut **tx)
                .await
                .context("作業場所の新規作成に失敗しました")?;
            Ok(name.clone())
        }
    }
}

/// トランザクション内で次の案件IDを採番(PRJ + 8桁ゼロ埋め連番)。
/// master_repo.rs::generate_next_id() と同じ採番ロジックだが、
/// レポジトリ層の独立性を保つためproject_repo内に複製する(ボーイスカウトルールの範囲内の重複として許容)。
async fn generate_next_project_id(tx: &mut Transaction<'_, Postgres>) -> Result<String> {
    let next_val: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(CAST(SUBSTRING(project_id FROM 4) AS BIGINT)), 0) + 1 FROM m_project WHERE project_id LIKE 'PRJ%'"
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(format!("PRJ{:0>8}", next_val))
}

pub struct WizardProjectInput {
    pub client_id: i64,
    pub name: String,
    pub description: String,
    pub report_deadline_type: String,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    pub report_request_day: Option<i32>,
}

/// クライアント契約ステップはスキップ可能(2026-07-31確定)。
pub struct WizardClientContractInput {
    pub engineer: EngineerRef,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub settlement_type: String,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub base_rate: i32,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub effort: Decimal,
    pub mid_month_rule: Option<String>,
    pub billing_timing: Option<String>,
    pub payment_terms: Option<String>,
    pub report_deadline_days_before: Option<i32>,
    pub currency: Option<String>,
    pub remarks: Option<String>,
}

/// パートナー契約は0件以上のリスト(2026-07-31確定)。
pub struct WizardPartnerContractInput {
    pub partner_id: String,
    pub engineer: EngineerRef,
    pub work_location: WorkLocationRef,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub settlement_type: String,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub base_rate: i32,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub effort: Decimal,
    pub mid_month_rule: Option<String>,
    pub kou_responsible: Option<String>,
    pub kou_contact: Option<String>,
    pub otsu_responsible: Option<String>,
    pub otsu_contact: Option<String>,
    pub work_responsible: Option<String>,
    pub deliverable_text: Option<String>,
    pub payment_condition: Option<String>,
    pub contract_items: Option<String>,
    pub remarks: Option<String>,
    pub order_create_deadline_day: Option<i32>,
    pub order_approve_deadline_days_before: Option<i32>,
    pub report_upload_deadline_days_before: Option<i32>,
    pub invoice_create_deadline_day: Option<i32>,
    pub invoice_approve_deadline_day: Option<i32>,
}

pub struct WizardInput {
    pub project: WizardProjectInput,
    pub client_contract: Option<WizardClientContractInput>,
    pub partner_contracts: Vec<WizardPartnerContractInput>,
}

/// 案件作成ウィザードのメイン処理。1トランザクション内で案件・クライアント契約(任意)・
/// パートナー契約(0件以上)をまとめて作成する。途中で何か失敗したら全体をROLLBACKし、
/// どのステップで失敗したかが分かるメッセージを返す。作成された案件IDを返す。
pub async fn create_project_wizard(pool: &PgPool, input: WizardInput) -> Result<String> {
    let mut tx = pool.begin().await?;

    let project_id = generate_next_project_id(&mut tx).await?;

    sqlx::query(
        r#"
        INSERT INTO m_project (
            project_id, client_id, name, description, is_active,
            report_deadline_type, report_deadline_value, report_deadline_holiday_rule, report_request_day
        ) VALUES ($1, $2, $3, $4, true, $5, $6, $7, $8)
        "#
    )
    .bind(&project_id)
    .bind(input.project.client_id)
    .bind(&input.project.name)
    .bind(&input.project.description)
    .bind(&input.project.report_deadline_type)
    .bind(input.project.report_deadline_value)
    .bind(&input.project.report_deadline_holiday_rule)
    .bind(input.project.report_request_day)
    .execute(&mut *tx)
    .await
    .context("案件の作成に失敗しました")?;

    if let Some(cc) = &input.client_contract {
        let engineer_id = resolve_engineer(&mut tx, &cc.engineer).await?;

        sqlx::query(
            r#"
            INSERT INTO m_client_contract (
                project_id, engineer_id, start_date, end_date,
                settlement_type, base_rate, effort,
                lower_limit_hours, upper_limit_hours, fixed_hours,
                deduction_rate, overtime_rate,
                mid_month_rule, billing_timing, payment_terms,
                report_deadline_days_before, currency, remarks, is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, true)
            "#
        )
        .bind(&project_id)
        .bind(engineer_id)
        .bind(cc.start_date)
        .bind(cc.end_date)
        .bind(&cc.settlement_type)
        .bind(cc.base_rate)
        .bind(cc.effort)
        .bind(cc.lower_limit_hours)
        .bind(cc.upper_limit_hours)
        .bind(cc.fixed_hours)
        .bind(cc.deduction_rate)
        .bind(cc.overtime_rate)
        .bind(cc.mid_month_rule.as_deref().unwrap_or("FULL_MONTH"))
        .bind(cc.billing_timing.as_deref().unwrap_or("MONTHLY"))
        .bind(cc.payment_terms.as_deref().unwrap_or(""))
        .bind(cc.report_deadline_days_before.unwrap_or(5))
        .bind(cc.currency.as_deref().unwrap_or("JPY"))
        .bind(cc.remarks.as_deref().unwrap_or(""))
        .execute(&mut *tx)
        .await
        .context("クライアント契約の作成に失敗しました")?;
    }

    for pc in &input.partner_contracts {
        let engineer_id = resolve_engineer(&mut tx, &pc.engineer).await?;
        let work_location_name = resolve_work_location(&mut tx, &pc.work_location).await?;

        sqlx::query(
            r#"
            INSERT INTO m_partner_contract (
                project_id, engineer_id, partner_id, start_date, end_date,
                settlement_type, lower_limit_hours, upper_limit_hours,
                fixed_hours, base_rate, deduction_rate, overtime_rate,
                effort, mid_month_rule,
                "甲_責任者", "甲_担当者", "乙_責任者", "乙_担当者", "作業責任者",
                deliverable_text, payment_condition, contract_items, work_location, remarks,
                order_create_deadline_day, order_approve_deadline_days_before,
                report_upload_deadline_days_before, invoice_create_deadline_day,
                invoice_approve_deadline_day,
                is_active
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                      $15, $16, $17, $18, $19, $20, $21, $22, $23, $24,
                      $25, $26, $27, $28, $29, true)
            "#
        )
        .bind(&project_id)
        .bind(engineer_id)
        .bind(&pc.partner_id)
        .bind(pc.start_date)
        .bind(pc.end_date)
        .bind(&pc.settlement_type)
        .bind(pc.lower_limit_hours)
        .bind(pc.upper_limit_hours)
        .bind(pc.fixed_hours)
        .bind(pc.base_rate)
        .bind(pc.deduction_rate)
        .bind(pc.overtime_rate)
        .bind(pc.effort)
        .bind(pc.mid_month_rule.as_deref().unwrap_or("FULL_MONTH"))
        .bind(pc.kou_responsible.as_deref().unwrap_or(""))
        .bind(pc.kou_contact.as_deref().unwrap_or(""))
        .bind(pc.otsu_responsible.as_deref().unwrap_or(""))
        .bind(pc.otsu_contact.as_deref().unwrap_or(""))
        .bind(pc.work_responsible.as_deref().unwrap_or(""))
        .bind(pc.deliverable_text.as_deref().unwrap_or(""))
        .bind(pc.payment_condition.as_deref().unwrap_or(""))
        .bind(pc.contract_items.as_deref().unwrap_or(""))
        .bind(&work_location_name)
        .bind(pc.remarks.as_deref().unwrap_or(""))
        .bind(pc.order_create_deadline_day.unwrap_or(15))
        .bind(pc.order_approve_deadline_days_before.unwrap_or(0))
        .bind(pc.report_upload_deadline_days_before.unwrap_or(2))
        .bind(pc.invoice_create_deadline_day.unwrap_or(1))
        .bind(pc.invoice_approve_deadline_day.unwrap_or(10))
        .execute(&mut *tx)
        .await
        .context("パートナー契約の作成に失敗しました")?;
    }

    tx.commit().await?;
    Ok(project_id)
}
