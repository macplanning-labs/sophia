/// infrastructure/repositories/timesheet_repo.rs — 稼働報告CRUD
///
/// list_all, find_by_id は base.rs マクロで自動生成。
/// update_status, list_received_emails, count_pending は業務固有。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::timesheet::*;

// ── 共通CRUD（マクロ生成）──

impl_list_all!(list_all, MonthlyTimesheet, "t_monthly_timesheet", "target_month DESC, id DESC");
impl_find_by_id!(find_by_id, MonthlyTimesheet, "t_monthly_timesheet", "id");

// ── 業務固有 ──

/// 稼働報告 ステータス更新
pub async fn update_status(pool: &PgPool, id: i64, status: &str) -> Result<()> {
    sqlx::query(
        "UPDATE t_monthly_timesheet SET status = $1, updated_at = NOW() WHERE id = $2"
    )
    .bind(status)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 受信メール一覧
pub async fn list_received_emails(pool: &PgPool) -> Result<Vec<ReceivedEmail>> {
    let rows = sqlx::query_as::<_, ReceivedEmail>(
        "SELECT * FROM t_received_email ORDER BY received_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 未承認の稼働報告件数
pub async fn count_pending(pool: &PgPool) -> Result<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM t_monthly_timesheet WHERE status IN ('UPLOADED', 'PARSED')"
    )
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

// ── パートナー稼働報告アップロード（2026-07-13追加。P2-3続き:
//    presentation/handlers/partner_timesheet.rs 直書きSQLのRepository層移行）──

/// パートナーID・技術者名・対象月から、同一案件の受注契約を解決する。
/// 対象月に期間が重なる契約のみ候補とする。0件は None、複数は Err(件数)。
pub async fn find_client_contract_id_by_partner_worker_month(
    pool: &PgPool,
    partner_id: &str,
    worker_name_normalized: &str,
    target_month: chrono::NaiveDate,
) -> Result<Result<Option<i64>, i64>> {
    let month_end = next_month_start(target_month);
    let ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT cc.id
          FROM m_client_contract cc
          JOIN m_engineer e ON cc.engineer_id = e.id
         WHERE REPLACE(REPLACE(e.name, '　', ''), ' ', '') = $2
           AND cc.start_date < $4 AND cc.end_date >= $3
           AND EXISTS (
                SELECT 1
                  FROM m_partner_contract pc
                 WHERE pc.engineer_id = e.id
                   AND pc.partner_id = $1
                   AND pc.project_id = cc.project_id
                   AND pc.start_date < $4 AND pc.end_date >= $3
           )
         ORDER BY cc.is_active DESC, cc.start_date DESC, cc.id DESC
        "#,
    )
    .bind(partner_id)
    .bind(worker_name_normalized)
    .bind(target_month)
    .bind(month_end)
    .fetch_all(pool)
    .await?;

    Ok(match ids.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        many => Err(many.len() as i64),
    })
}

/// 後方互換: 対象月なしの旧API（最新開始日の1件）。新規呼び出しは月付き版を使う。
#[allow(dead_code)]
pub async fn find_client_contract_id_by_partner_and_worker(
    pool: &PgPool,
    partner_id: &str,
    worker_name_normalized: &str,
) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar(
        r#"SELECT cc.id FROM m_client_contract cc
           JOIN m_engineer e ON cc.engineer_id = e.id
           JOIN m_partner_contract pc ON pc.engineer_id = e.id AND pc.project_id = cc.project_id
           WHERE pc.partner_id = $1
             AND REPLACE(REPLACE(e.name, '　', ''), ' ', '') = $2
           ORDER BY cc.start_date DESC LIMIT 1"#
    )
    .bind(partner_id)
    .bind(worker_name_normalized)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

fn next_month_start(month: chrono::NaiveDate) -> chrono::NaiveDate {
    use chrono::Datelike;
    if month.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(month.year() + 1, 1, 1).unwrap_or(month)
    } else {
        chrono::NaiveDate::from_ymd_opt(month.year(), month.month() + 1, 1).unwrap_or(month)
    }
}

/// 稼働報告アップロード内容をupsertする（同一契約・対象月の既存レコードは更新。statusは'UPLOADED'または'PARSED'）
#[allow(clippy::too_many_arguments)]
pub async fn upsert_monthly_timesheet_upload(
    pool: &PgPool,
    client_contract_id: i64,
    target_month: chrono::NaiveDate,
    total_hours: rust_decimal::Decimal,
    work_days: i32,
    overtime_hours: rust_decimal::Decimal,
    night_hours: rust_decimal::Decimal,
    holiday_hours: rust_decimal::Decimal,
    original_filename: &str,
    daily_data: &serde_json::Value,
    alerts_json: &serde_json::Value,
    status: &str,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO t_monthly_timesheet (
            client_contract_id, target_month, status,
            total_hours, work_days, overtime_hours, night_hours, holiday_hours,
            original_filename, daily_data, alerts_json, uploaded_at
        ) VALUES ($1, $2, $11, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
        ON CONFLICT (client_contract_id, target_month)
        DO UPDATE SET
            total_hours = EXCLUDED.total_hours,
            work_days = EXCLUDED.work_days,
            overtime_hours = EXCLUDED.overtime_hours,
            night_hours = EXCLUDED.night_hours,
            holiday_hours = EXCLUDED.holiday_hours,
            original_filename = EXCLUDED.original_filename,
            daily_data = EXCLUDED.daily_data,
            alerts_json = EXCLUDED.alerts_json,
            uploaded_at = NOW(),
            status = $11"#
    )
    .bind(client_contract_id)
    .bind(target_month)
    .bind(total_hours)
    .bind(work_days)
    .bind(overtime_hours)
    .bind(night_hours)
    .bind(holiday_hours)
    .bind(original_filename)
    .bind(daily_data)
    .bind(alerts_json)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(())
}

/// パートナーポータル向け稼働報告一覧（id, 対象月, ステータス, 技術者名, 総稼働時間）
pub async fn list_timesheets_for_partner(pool: &PgPool, partner_id: &str) -> Result<Vec<(i64, String, String, String, String)>> {
    let rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(
        r#"SELECT mt.id, mt.target_month::TEXT, mt.status, COALESCE(me.name, ''), COALESCE(mt.total_hours::TEXT, '0')
         FROM t_monthly_timesheet mt
         JOIN m_client_contract cc ON mt.client_contract_id = cc.id
         JOIN m_engineer me ON cc.engineer_id = me.id
         JOIN m_partner_contract pc ON pc.engineer_id = me.id AND pc.is_active = true
         WHERE pc.partner_id = $1
         ORDER BY mt.target_month DESC"#
    )
    .bind(partner_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナーポータルから稼働報告を提出済み(SUBMITTED)にする（当該パートナーの契約に紐づく場合のみ。更新件数を返す）
pub async fn submit_timesheet_for_partner(pool: &PgPool, id: i64, partner_id: &str) -> Result<u64> {
    let result = sqlx::query(
        r#"UPDATE t_monthly_timesheet SET status = 'SUBMITTED'
        WHERE id = $1 AND status = 'UPLOADED'
          AND client_contract_id IN (
            SELECT cc.id FROM m_client_contract cc
            JOIN m_partner_contract pc ON pc.engineer_id = cc.engineer_id AND pc.is_active = true
            WHERE pc.partner_id = $2
          )"#
    )
    .bind(id)
    .bind(partner_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

// ── 稼働報告 承認・差戻し・送付・アップロード（2026-07-13追加。P2-3続き:
//    presentation/handlers/timesheets.rs 直書きSQLのRepository層移行）──

/// 稼働報告確定通知メール用の情報
#[derive(sqlx::FromRow)]
pub struct TimesheetReportInfo {
    pub worker_name: Option<String>,
    pub work_month: Option<chrono::NaiveDate>,
    pub total_hours: Option<f64>,
    pub work_days: Option<i32>,
    pub partner_name: Option<String>,
    pub partner_email: Option<String>,
}

/// 稼働報告を承認する（UPLOADED/PARSEDのもののみ。更新件数を返す）
pub async fn approve(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_monthly_timesheet SET status = 'APPROVED', updated_at = NOW() WHERE id = $1 AND status IN ('UPLOADED', 'PARSED')"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 承認通知メール用の稼働報告情報を取得する
pub async fn find_report_info_for_notification(pool: &PgPool, id: i64) -> Result<Option<TimesheetReportInfo>> {
    let info = sqlx::query_as::<_, TimesheetReportInfo>(
        r#"SELECT e.name AS worker_name,
                  mt.target_month AS work_month,
                  mt.total_hours::float8 AS total_hours,
                  mt.work_days,
                  p.name AS partner_name,
                  NULLIF(p.email, '') AS partner_email
           FROM t_monthly_timesheet mt
           LEFT JOIN m_client_contract cc ON cc.id = mt.client_contract_id
           LEFT JOIN m_engineer e ON e.id = cc.engineer_id
           LEFT JOIN m_partner p ON p.partner_id = e.partner_id
           WHERE mt.id = $1"#
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(info)
}

/// 稼働報告を差戻す（PENDINGに戻す）
pub async fn reject(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_monthly_timesheet SET status = 'PENDING', updated_at = NOW() WHERE id = $1"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 稼働報告をクライアント送付済みにする（APPROVEDのもののみ）
pub async fn send_to_client(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_monthly_timesheet SET status = 'SENT', sent_to_client_at = NOW(), updated_at = NOW() WHERE id = $1 AND status = 'APPROVED'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// エンジニア名・対象月から受注契約を解決する。
/// 対象月に期間が重なる契約のみ。0件は None、複数は Err(件数)。
pub async fn find_client_contract_id_by_engineer_name_month(
    pool: &PgPool,
    worker_name_normalized: &str,
    target_month: chrono::NaiveDate,
) -> Result<Result<Option<i64>, i64>> {
    let month_end = next_month_start(target_month);
    let ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT cc.id
          FROM m_client_contract cc
          JOIN m_engineer e ON cc.engineer_id = e.id
         WHERE REPLACE(REPLACE(e.name, '　', ''), ' ', '') = $1
           AND cc.start_date < $3 AND cc.end_date >= $2
         ORDER BY cc.is_active DESC, cc.start_date DESC, cc.id DESC
        "#,
    )
    .bind(worker_name_normalized)
    .bind(target_month)
    .bind(month_end)
    .fetch_all(pool)
    .await?;

    Ok(match ids.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        many => Err(many.len() as i64),
    })
}

/// エンジニア名 LIKE + 対象月。完全一致で0件のときのフォールバック。
pub async fn find_client_contract_id_by_engineer_name_like_month(
    pool: &PgPool,
    worker_name: &str,
    target_month: chrono::NaiveDate,
) -> Result<Result<Option<i64>, i64>> {
    let month_end = next_month_start(target_month);
    let ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT cc.id
          FROM m_client_contract cc
          JOIN m_engineer e ON cc.engineer_id = e.id
         WHERE e.name LIKE $1
           AND cc.start_date < $3 AND cc.end_date >= $2
         ORDER BY cc.is_active DESC, cc.start_date DESC, cc.id DESC
        "#,
    )
    .bind(format!("%{}%", worker_name))
    .bind(target_month)
    .bind(month_end)
    .fetch_all(pool)
    .await?;

    Ok(match ids.as_slice() {
        [] => Ok(None),
        [id] => Ok(Some(*id)),
        many => Err(many.len() as i64),
    })
}

/// 明示指定の受注契約が、対象月と期間が重なるか確認する。
pub async fn client_contract_covers_month(
    pool: &PgPool,
    client_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<bool> {
    let month_end = next_month_start(target_month);
    let ok: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM m_client_contract cc
             WHERE cc.id = $1
               AND cc.start_date < $3 AND cc.end_date >= $2
        )
        "#,
    )
    .bind(client_contract_id)
    .bind(target_month)
    .bind(month_end)
    .fetch_one(pool)
    .await?;
    Ok(ok)
}

/// エンジニア名（空白除去済み）完全一致で最新の顧客契約IDを検索する（対象月なし・後方互換）
#[allow(dead_code)]
pub async fn find_client_contract_id_by_engineer_name(pool: &PgPool, worker_name_normalized: &str) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar(
        r#"SELECT cc.id FROM m_client_contract cc
           JOIN m_engineer e ON cc.engineer_id = e.id
           WHERE REPLACE(REPLACE(e.name, '　', ''), ' ', '') = $1
           ORDER BY cc.start_date DESC LIMIT 1"#
    )
    .bind(worker_name_normalized)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// エンジニア名の部分一致（LIKE）で最新の顧客契約IDを検索する（後方互換）
#[allow(dead_code)]
pub async fn find_client_contract_id_by_engineer_name_like(pool: &PgPool, worker_name: &str) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar(
        r#"SELECT cc.id FROM m_client_contract cc
           JOIN m_engineer e ON cc.engineer_id = e.id
           WHERE e.name LIKE $1
           ORDER BY cc.start_date DESC LIMIT 1"#
    )
    .bind(format!("%{}%", worker_name))
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// 受注契約・対象月に紐づく受注書（t_received_order）が存在するかを判定する。
///
/// EDI取込の受注は `client_contract_id` が空のことがあるため、次のいずれかで一致すれば可とする:
/// 1. `client_contract_id` が当該契約と一致
/// 2. 技術者・対象月・クライアント・案件名が契約と一致（注文書に載る4項目）
///
/// CANCELLED は未整備扱いとする。
pub async fn exists_received_order_for_contract_month(
    pool: &PgPool,
    client_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1
              FROM t_received_order ro
              JOIN m_client_contract cc ON cc.id = $1
              JOIN m_project pr ON pr.project_id = cc.project_id
             WHERE ro.target_month = $2
               AND COALESCE(ro.status, '') <> 'CANCELLED'
               AND ro.client_id = pr.client_id
               AND (
                    ro.client_contract_id = cc.id
                    OR (
                        ro.engineer_id = cc.engineer_id
                        AND (
                            BTRIM(COALESCE(ro.project_name, '')) = BTRIM(pr.name)
                            OR (
                                NULLIF(BTRIM(COALESCE(pr.edi_project_alias, '')), '') IS NOT NULL
                                AND BTRIM(ro.project_name) = BTRIM(pr.edi_project_alias)
                            )
                        )
                    )
               )
        )"#
    )
    .bind(client_contract_id)
    .bind(target_month)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// 稼働報告一覧表示用の行
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct TimesheetRow {
    pub id: i64,
    pub target_month: chrono::NaiveDate,
    pub status: String,
    pub total_hours: rust_decimal::Decimal,
    pub work_days: i32,
    pub contract_info: String,
    pub engineer_name: String,
    pub original_filename: String,
}

/// 稼働報告一覧（契約・エンジニア名JOIN済み、最大100件）
pub async fn list_timesheet_rows(pool: &PgPool) -> Result<Vec<TimesheetRow>> {
    let rows = sqlx::query_as::<_, TimesheetRow>(
        r#"
        SELECT ts.id, ts.target_month, ts.status, ts.total_hours, ts.work_days,
               CASE WHEN ts.employee_id IS NOT NULL THEN '自己申告' ELSE COALESCE(cc.settlement_type, '—') END AS contract_info,
               COALESCE(e.name, emp.last_name || ' ' || emp.first_name, '—') AS engineer_name,
               COALESCE(ts.original_filename, '') AS original_filename
        FROM t_monthly_timesheet ts
        LEFT JOIN m_client_contract cc ON cc.id = ts.client_contract_id
        LEFT JOIN m_engineer e ON e.id = cc.engineer_id
        LEFT JOIN m_employee emp ON emp.id = ts.employee_id
        ORDER BY ts.target_month DESC, ts.id DESC LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 契約の表示用文字列（クライアント名 / 精算方式）を取得する
pub async fn find_contract_display_info(pool: &PgPool, client_contract_id: i64) -> Result<String> {
    let info: String = sqlx::query_scalar(
        r#"SELECT COALESCE(c.name, '(不明)') || ' / ' || COALESCE(cc.settlement_type, '')
           FROM m_client_contract cc
           JOIN m_project p ON p.project_id = cc.project_id
           JOIN m_client c ON c.id = p.client_id
           WHERE cc.id = $1"#
    )
    .bind(client_contract_id)
    .fetch_one(pool)
    .await?;
    Ok(info)
}

/// 社員自己申告の表示用文字列（氏名）を取得する
pub async fn find_employee_display_info(pool: &PgPool, employee_id: i64) -> Result<String> {
    let name: String = sqlx::query_scalar(
        "SELECT last_name || ' ' || first_name FROM m_employee WHERE id = $1"
    )
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    Ok(format!("{}（社員自己申告）", name))
}

/// 稼働報告に紐づくメール履歴（t_email_log）一覧
pub async fn list_source_emails_for_timesheet(pool: &PgPool, timesheet_id: i64) -> Result<Vec<(String, String, String, String)>> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"SELECT COALESCE(from_name, from_email), subject,
               COALESCE(received_at::text, ''), status
           FROM t_email_log
           WHERE timesheet_id = $1
           ORDER BY received_at DESC"#
    )
    .bind(timesheet_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── 日次稼働報告(t_daily_work_entry)（2026-07-13追加。P2-3続き:
//    presentation/handlers/daily_work_entry.rs 直書きSQLのRepository層移行）──

/// 日次稼働報告 1件
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct WorkEntryRow {
    pub id: i64,
    pub engineer_id: i64,
    pub work_date: chrono::NaiveDate,
    pub start_time: Option<chrono::NaiveTime>,
    pub end_time: Option<chrono::NaiveTime>,
    pub break_minutes: i32,
    pub actual_minutes: i32,
    pub work_description: String,
    pub is_holiday: bool,
}

/// 指定エンジニア・期間の日次稼働報告一覧
pub async fn list_work_entries(pool: &PgPool, engineer_id: i64, start_date: chrono::NaiveDate, end_date: chrono::NaiveDate) -> Result<Vec<WorkEntryRow>> {
    let rows = sqlx::query_as::<_, WorkEntryRow>(
        r#"
        SELECT id, engineer_id, work_date, start_time, end_time,
               break_minutes, actual_minutes, work_description, is_holiday
        FROM t_daily_work_entry
        WHERE engineer_id = $1 AND work_date >= $2 AND work_date < $3
        ORDER BY work_date
        "#
    )
    .bind(engineer_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 指定エンジニア・対象月の稼働報告がロック済み（確定済み）か
pub async fn is_timesheet_locked(pool: &PgPool, engineer_id: i64, target_month: chrono::NaiveDate) -> Result<bool> {
    let locked_at: Option<Option<chrono::DateTime<chrono::Utc>>> = sqlx::query_scalar(
        r#"
        SELECT ts.locked_at
        FROM t_monthly_timesheet ts
        JOIN m_client_contract cc ON cc.id = ts.client_contract_id
        WHERE cc.engineer_id = $1 AND ts.target_month = $2
        LIMIT 1
        "#
    )
    .bind(engineer_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await?;
    Ok(locked_at.flatten().is_some())
}

/// 日次稼働報告をupsertする
#[allow(clippy::too_many_arguments)]
pub async fn upsert_work_entry(
    pool: &PgPool,
    engineer_id: i64,
    work_date: chrono::NaiveDate,
    start_time: Option<chrono::NaiveTime>,
    end_time: Option<chrono::NaiveTime>,
    break_minutes: i32,
    actual_minutes: i32,
    work_description: &str,
    is_holiday: bool,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_daily_work_entry
            (engineer_id, work_date, start_time, end_time, break_minutes, actual_minutes, work_description, is_holiday, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
        ON CONFLICT (engineer_id, work_date) DO UPDATE SET
            start_time = EXCLUDED.start_time,
            end_time = EXCLUDED.end_time,
            break_minutes = EXCLUDED.break_minutes,
            actual_minutes = EXCLUDED.actual_minutes,
            work_description = EXCLUDED.work_description,
            is_holiday = EXCLUDED.is_holiday,
            updated_at = NOW()
        "#
    )
    .bind(engineer_id)
    .bind(work_date)
    .bind(start_time)
    .bind(end_time)
    .bind(break_minutes)
    .bind(actual_minutes)
    .bind(work_description)
    .bind(is_holiday)
    .execute(pool)
    .await?;
    Ok(())
}

/// 指定期間の稼働日数・合計稼働分数
pub async fn sum_work_minutes(pool: &PgPool, engineer_id: i64, start_date: chrono::NaiveDate, end_date: chrono::NaiveDate) -> Result<(i64, i64)> {
    let row: (i64, Option<i64>) = sqlx::query_as(
        r#"
        SELECT COUNT(*) as work_days,
               COALESCE(SUM(actual_minutes), 0) as total_minutes
        FROM t_daily_work_entry
        WHERE engineer_id = $1
          AND work_date >= $2
          AND work_date < $3
          AND is_holiday = false
          AND actual_minutes > 0
        "#
    )
    .bind(engineer_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_one(pool)
    .await?;
    Ok((row.0, row.1.unwrap_or(0)))
}

/// 指定期間で入力済み（稼働あり or 休日指定あり）の日付一覧
pub async fn list_entered_dates(pool: &PgPool, engineer_id: i64, start_date: chrono::NaiveDate, end_date: chrono::NaiveDate) -> Result<Vec<chrono::NaiveDate>> {
    let rows: Vec<chrono::NaiveDate> = sqlx::query_scalar(
        r#"
        SELECT work_date FROM t_daily_work_entry
        WHERE engineer_id = $1
          AND work_date >= $2
          AND work_date < $3
          AND (actual_minutes > 0 OR is_holiday = true)
        ORDER BY work_date
        "#
    )
    .bind(engineer_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── 社員自己申告（案件非依存の月次稼働報告、2026-07-30追加）──

/// 社員×対象月の既存の自己申告を取得する（再編集用）
pub async fn find_self_report(pool: &PgPool, employee_id: i64, target_month: chrono::NaiveDate) -> Result<Option<MonthlyTimesheet>> {
    let row = sqlx::query_as::<_, MonthlyTimesheet>(
        "SELECT * FROM t_monthly_timesheet WHERE employee_id = $1 AND target_month = $2"
    )
    .bind(employee_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Googleスプレッドシート連携: 複製したテンプレートのファイルIDを紐付ける（未提出の下書き状態として作成/更新）
pub async fn upsert_self_report_sheet_link(
    pool: &PgPool,
    employee_id: i64,
    target_month: chrono::NaiveDate,
    sheet_file_id: &str,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"INSERT INTO t_monthly_timesheet (employee_id, target_month, status, sheet_file_id)
           VALUES ($1, $2, 'PENDING', $3)
           ON CONFLICT (employee_id, target_month) WHERE employee_id IS NOT NULL
           DO UPDATE SET sheet_file_id = EXCLUDED.sheet_file_id, updated_at = NOW()
           RETURNING id"#
    )
    .bind(employee_id)
    .bind(target_month)
    .bind(sheet_file_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 社員自己申告の内容をupsertする（呼び出し側で承認済み月への再提出を防ぐこと）
#[allow(clippy::too_many_arguments)]
pub async fn upsert_self_report(
    pool: &PgPool,
    employee_id: i64,
    target_month: chrono::NaiveDate,
    daily_data: &serde_json::Value,
    total_hours: rust_decimal::Decimal,
    work_days: i32,
    overtime_hours: rust_decimal::Decimal,
    night_hours: rust_decimal::Decimal,
    holiday_hours: rust_decimal::Decimal,
    uploaded_by_id: i64,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"INSERT INTO t_monthly_timesheet (
               employee_id, target_month, status, daily_data,
               total_hours, work_days, overtime_hours, night_hours, holiday_hours,
               uploaded_by_id, uploaded_at
           )
           VALUES ($1, $2, 'UPLOADED', $3, $4, $5, $6, $7, $8, $9, NOW())
           ON CONFLICT (employee_id, target_month) WHERE employee_id IS NOT NULL
           DO UPDATE SET
               status = 'UPLOADED',
               daily_data = EXCLUDED.daily_data,
               total_hours = EXCLUDED.total_hours,
               work_days = EXCLUDED.work_days,
               overtime_hours = EXCLUDED.overtime_hours,
               night_hours = EXCLUDED.night_hours,
               holiday_hours = EXCLUDED.holiday_hours,
               uploaded_by_id = EXCLUDED.uploaded_by_id,
               uploaded_at = NOW(),
               updated_at = NOW()
           RETURNING id"#
    )
    .bind(employee_id)
    .bind(target_month)
    .bind(daily_data)
    .bind(total_hours)
    .bind(work_days)
    .bind(overtime_hours)
    .bind(night_hours)
    .bind(holiday_hours)
    .bind(uploaded_by_id)
    .fetch_one(pool)
    .await?;
    Ok(id)
}
