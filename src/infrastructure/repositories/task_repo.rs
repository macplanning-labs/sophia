/// infrastructure/repositories/task_repo.rs — タスクCRUD

use anyhow::Result;
use chrono::NaiveDate;
use sqlx::PgPool;

use crate::domain::models::task::MonthlyTask;

/// タスク一覧（月指定）
pub async fn list_by_month(pool: &PgPool, work_month: NaiveDate) -> Result<Vec<MonthlyTask>> {
    let rows = sqlx::query_as::<_, MonthlyTask>(
        "SELECT * FROM t_monthly_task WHERE work_month = $1 ORDER BY deadline, task_type"
    )
    .bind(work_month)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// タスク完了
pub async fn complete(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_monthly_task SET status = 'DONE', completed_at = NOW(), updated_at = NOW() WHERE id = $1"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 期限超過タスク件数
pub async fn count_overdue(pool: &PgPool) -> Result<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM t_monthly_task WHERE status NOT IN ('DONE', 'SKIPPED') AND deadline < CURRENT_DATE"
    )
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

/// 今月の未完了タスク件数
pub async fn count_pending_this_month(pool: &PgPool) -> Result<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM t_monthly_task WHERE status NOT IN ('DONE', 'SKIPPED') AND work_month = DATE_TRUNC('month', CURRENT_DATE)"
    )
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

// ── 月次タスク一括生成（2026-07-13追加。P2-3続き:
//    presentation/handlers/tasks.rs 直書きSQLのRepository層移行）──

/// 有効契約のあるエンジニア一覧（id, project_id, name）— 月次タスク一括生成用
/// 月次タスク生成対象のエンジニア×案件（稼働報告提出期限の解決に必要な設定込み）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EngineerTaskTarget {
    pub engineer_id: i64,
    pub project_id: String,
    pub engineer_name: String,
    /// 案件単位の提出期限設定（`m_project.report_deadline_*`）。未設定ならNoneのままフォールバック。
    pub report_deadline_type: Option<String>,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    /// 受注契約側のフォールバック値（該当契約が無い場合はデフォルト5）
    pub contract_deadline_days_before: Option<i32>,
}

pub async fn list_active_engineers_for_task_generation(pool: &PgPool) -> Result<Vec<EngineerTaskTarget>> {
    let rows: Vec<EngineerTaskTarget> = sqlx::query_as(
        r#"SELECT DISTINCT ON (e.id, pc.project_id)
                  e.id AS engineer_id, pc.project_id, e.name AS engineer_name,
                  pr.report_deadline_type, pr.report_deadline_value, pr.report_deadline_holiday_rule,
                  cc.report_deadline_days_before AS contract_deadline_days_before
           FROM m_engineer e
           JOIN m_partner_contract pc ON pc.engineer_id = e.id AND pc.is_active = true
           LEFT JOIN m_project pr ON pr.project_id = pc.project_id
           LEFT JOIN m_client_contract cc
             ON cc.project_id = pc.project_id AND cc.engineer_id = pc.engineer_id AND cc.is_active = true
           ORDER BY e.id, pc.project_id, e.name"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 指定条件のタスクが既に存在するか
pub async fn task_exists(pool: &PgPool, work_month: NaiveDate, task_type: &str, engineer_id: i64, project_id: &str) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_monthly_task WHERE work_month = $1 AND task_type = $2 AND engineer_id = $3 AND project_id = $4)"
    )
    .bind(work_month)
    .bind(task_type)
    .bind(engineer_id)
    .bind(project_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// タスクを新規作成する（ステータスはPENDING固定）
#[allow(clippy::too_many_arguments)]
pub async fn insert_task(
    pool: &PgPool,
    project_id: &str,
    engineer_id: i64,
    work_month: NaiveDate,
    task_type: &str,
    responsible: &str,
    deadline: NaiveDate,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO t_monthly_task (
            project_id, engineer_id, work_month, task_type,
            responsible, deadline, status, note
        ) VALUES ($1, $2, $3, $4, $5, $6, 'PENDING', '')"#
    )
    .bind(project_id)
    .bind(engineer_id)
    .bind(work_month)
    .bind(task_type)
    .bind(responsible)
    .bind(deadline)
    .execute(pool)
    .await?;
    Ok(())
}

/// タスクをスキップ済みにする
pub async fn skip(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_monthly_task SET status = 'SKIPPED', updated_at = NOW() WHERE id = $1"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// タスク一覧（締切昇順、最大200件。ダッシュボードAPI用）
pub async fn list_recent(pool: &PgPool, limit: i64) -> Result<Vec<MonthlyTask>> {
    let rows = sqlx::query_as::<_, MonthlyTask>(
        "SELECT * FROM t_monthly_task ORDER BY deadline ASC, id ASC LIMIT $1"
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 稼働報告提出リマインド対象（パートナー×対象月で集約）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PendingReportReminder {
    pub partner_id: String,
    pub partner_name: String,
    pub partner_email: String,
    pub target_month: String,
    pub work_month: NaiveDate,
    /// リンク生成用の代表契約ID（同一パートナー・同一月に契約が複数あれば最小のものを採用）。
    /// アップロード自体はリンク経由のトークンページで作業者名からパートナー内解決するため、
    /// この値でアップロード先契約が固定されるわけではない。
    pub partner_contract_id: i64,
    /// 提出期限（同一パートナー・同一月に複数タスクがあれば最も遅いものを採用）
    pub deadline: NaiveDate,
    /// 未登録エンジニア名を「、」区切りで連結したもの（メール本文で「○○さん、△△さん」と案内するため）
    pub engineer_names: String,
}

/// PENDING かつ未リマインドの REPORT_UPLOAD タスクをパートナー×月で集約して返す。
/// `lead_days`: 提出期限の何日前から対象にするか（期限を過ぎたタスクも常に対象）。
pub async fn list_pending_report_reminders(pool: &PgPool, lead_days: i32) -> Result<Vec<PendingReportReminder>> {
    let rows = sqlx::query_as::<_, PendingReportReminder>(
        r#"SELECT
              p.partner_id,
              COALESCE(p.name, '') AS partner_name,
              COALESCE(p.email, '') AS partner_email,
              TO_CHAR(DATE_TRUNC('month', mt.work_month), 'YYYY年MM月') AS target_month,
              DATE_TRUNC('month', mt.work_month)::date AS work_month,
              MIN(pc.id) AS partner_contract_id,
              MAX(mt.deadline) AS deadline,
              STRING_AGG(DISTINCT e.name, '、' ORDER BY e.name) AS engineer_names
           FROM t_monthly_task mt
           JOIN m_partner_contract pc
             ON pc.engineer_id = mt.engineer_id
            AND pc.project_id = mt.project_id
            AND pc.is_active = true
           JOIN m_partner p ON p.partner_id = pc.partner_id
           JOIN m_engineer e ON e.id = mt.engineer_id
           WHERE mt.task_type = 'REPORT_UPLOAD'
             AND mt.status = 'PENDING'
             AND mt.reminder_sent = false
             AND mt.deadline - $1 <= CURRENT_DATE
             AND p.email IS NOT NULL AND p.email != ''
           GROUP BY p.partner_id, p.name, p.email, DATE_TRUNC('month', mt.work_month)
           ORDER BY p.name, work_month"#
    )
    .bind(lead_days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 案件単位の稼働報告提出期限設定（プロジェクト設定 + 契約側フォールバック値）。
/// `list_active_engineers_for_task_generation` と同じ解決ロジックを、
/// 特定の project_id × engineer_id 1件分だけ取得するための軽量版。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReportDeadlineSettingsRow {
    pub report_deadline_type: Option<String>,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    pub contract_deadline_days_before: Option<i32>,
}

/// 指定の project_id × engineer_id の稼働報告提出期限設定を取得する
/// （クライアント契約側 `report_deadline_days_before` をフォールバック値として使用）。
pub async fn find_report_deadline_settings(
    pool: &PgPool,
    project_id: &str,
    engineer_id: i64,
) -> Result<Option<ReportDeadlineSettingsRow>> {
    let row = sqlx::query_as::<_, ReportDeadlineSettingsRow>(
        r#"SELECT pr.report_deadline_type, pr.report_deadline_value, pr.report_deadline_holiday_rule,
                  cc.report_deadline_days_before AS contract_deadline_days_before
           FROM m_project pr
           LEFT JOIN m_client_contract cc
             ON cc.project_id = pr.project_id AND cc.engineer_id = $2 AND cc.is_active = true
           WHERE pr.project_id = $1"#
    )
    .bind(project_id)
    .bind(engineer_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 指定パートナー・作業月のREPORT_UPLOADタスク提出期限を解決する（稼働報告 初回依頼メール用。
/// 手動送信ボタン・自動送信ジョブ共通）。
///
/// 1. 既にタスクが生成済みなら、その実際の期限をそのまま使う。
/// 2. タスク未生成の場合（初回依頼を月次タスク生成より先に送るケース）は、
///    月次タスク生成（tasks.rs）と同じ設定解決ロジック
///    （案件単位の report_deadline_type/value、未設定ならクライアント契約側の
///    report_deadline_days_before）で期限を計算する。
/// 3. 案件・契約のどちらにも設定が無い、またはengineer_id未設定の注文書の場合のみ、
///    「翌月15日」を最終フォールバックとする。
pub async fn resolve_report_deadline(
    pool: &PgPool,
    partner_id: &str,
    project_id: &str,
    engineer_id: Option<i64>,
    work_month: NaiveDate,
) -> NaiveDate {
    use crate::domain::services::business_day::{calculate_deadline, resolve_report_deadline_settings};
    use chrono::Datelike;

    if let Ok(Some(d)) = find_report_deadline(pool, partner_id, work_month).await {
        return d;
    }

    if let Some(engineer_id) = engineer_id {
        if let Ok(Some(settings_row)) = find_report_deadline_settings(pool, project_id, engineer_id).await {
            let settings = resolve_report_deadline_settings(
                settings_row.report_deadline_type.as_deref().unwrap_or("RELATIVE"),
                settings_row.report_deadline_value,
                settings_row.report_deadline_holiday_rule.as_deref(),
                settings_row.contract_deadline_days_before.unwrap_or(5),
            );
            if let Ok(d) = calculate_deadline(work_month.year(), work_month.month(), &settings) {
                return d;
            }
        }
    }

    work_month + chrono::Months::new(1) + chrono::Duration::days(14)
}

/// 指定パートナー・対象月の REPORT_UPLOAD タスク提出期限を取得する（稼働報告 初回依頼メール用）。
/// 複数エンジニアの契約があれば最も遅い期限を採用。タスクがまだ存在しない場合は None。
pub async fn find_report_deadline(pool: &PgPool, partner_id: &str, work_month: NaiveDate) -> Result<Option<NaiveDate>> {
    let deadline: Option<NaiveDate> = sqlx::query_scalar(
        r#"SELECT MAX(mt.deadline)
           FROM t_monthly_task mt
           JOIN m_partner_contract pc
             ON pc.engineer_id = mt.engineer_id
            AND pc.project_id = mt.project_id
            AND pc.is_active = true
           WHERE mt.task_type = 'REPORT_UPLOAD'
             AND mt.work_month = $1
             AND pc.partner_id = $2"#
    )
    .bind(work_month)
    .bind(partner_id)
    .fetch_one(pool)
    .await?;
    Ok(deadline)
}

/// 指定パートナー・対象月の REPORT_UPLOAD タスクに reminder_sent = true を立てる
pub async fn mark_report_reminder_sent(
    pool: &PgPool,
    partner_id: &str,
    work_month: NaiveDate,
) -> Result<u64> {
    let result = sqlx::query(
        r#"UPDATE t_monthly_task mt
           SET reminder_sent = true, updated_at = NOW()
           FROM m_partner_contract pc
           WHERE mt.engineer_id = pc.engineer_id
             AND mt.project_id = pc.project_id
             AND pc.partner_id = $1
             AND pc.is_active = true
             AND mt.task_type = 'REPORT_UPLOAD'
             AND mt.status = 'PENDING'
             AND DATE_TRUNC('month', mt.work_month) = DATE_TRUNC('month', $2::date)"#
    )
    .bind(partner_id)
    .bind(work_month)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
