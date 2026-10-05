/// infrastructure/repositories/payroll_repo.rs — 給与CRUD

use anyhow::Result;
use chrono::NaiveDate;
use sqlx::PgPool;

use crate::domain::models::payroll::*;

/// 社員一覧（有効のみ）
pub async fn list_employees(pool: &PgPool) -> Result<Vec<Employee>> {
    let rows = sqlx::query_as::<_, Employee>(
        "SELECT * FROM m_employee WHERE is_active = true ORDER BY employee_id"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 社員取得
pub async fn find_employee(pool: &PgPool, id: i64) -> Result<Option<Employee>> {
    let row = sqlx::query_as::<_, Employee>(
        "SELECT * FROM m_employee WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 給与一覧（月指定）
pub async fn list_payrolls(pool: &PgPool, year_month: NaiveDate) -> Result<Vec<PayrollWithEmployee>> {
    let rows = sqlx::query_as::<_, PayrollWithEmployee>(
        r#"
        SELECT p.id, p.employee_id, p.year_month, p.status,
               p.gross_pay, p.deduction_total, p.net_pay,
               e.employee_id as employee_code, e.last_name, e.first_name
        FROM t_payroll p
        JOIN m_employee e ON p.employee_id = e.id
        WHERE p.year_month = $1
        ORDER BY e.employee_id
        "#
    )
    .bind(year_month)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 給与取得
pub async fn find_payroll(pool: &PgPool, id: i64) -> Result<Option<Payroll>> {
    let row = sqlx::query_as::<_, Payroll>(
        "SELECT * FROM t_payroll WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 給与Upsert
pub async fn upsert_payroll(pool: &PgPool, data: &crate::domain::services::payroll_calculator::PayrollData) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_payroll (
            employee_id, year_month, status,
            work_days, total_hours, overtime_hours, night_hours, holiday_hours, absence_days,
            base_salary, position_allowance, housing_allowance, commuting_allowance,
            overtime_pay, night_pay, holiday_pay, absence_deduction, gross_pay,
            pension_premium, health_premium, nursing_premium, employment_premium,
            social_insurance_total, income_tax, resident_tax, deduction_total, net_pay
        ) VALUES (
            $1, $2, 'DRAFT',
            $3, $4, $5, $6, $7, $8,
            $9, $10, $11, $12,
            $13, $14, $15, $16, $17,
            $18, $19, $20, $21,
            $22, $23, $24, $25, $26
        )
        ON CONFLICT (employee_id, year_month) DO UPDATE SET
            work_days = EXCLUDED.work_days, total_hours = EXCLUDED.total_hours,
            overtime_hours = EXCLUDED.overtime_hours, night_hours = EXCLUDED.night_hours,
            holiday_hours = EXCLUDED.holiday_hours, absence_days = EXCLUDED.absence_days,
            base_salary = EXCLUDED.base_salary, position_allowance = EXCLUDED.position_allowance,
            housing_allowance = EXCLUDED.housing_allowance, commuting_allowance = EXCLUDED.commuting_allowance,
            overtime_pay = EXCLUDED.overtime_pay, night_pay = EXCLUDED.night_pay,
            holiday_pay = EXCLUDED.holiday_pay, absence_deduction = EXCLUDED.absence_deduction,
            gross_pay = EXCLUDED.gross_pay,
            pension_premium = EXCLUDED.pension_premium, health_premium = EXCLUDED.health_premium,
            nursing_premium = EXCLUDED.nursing_premium, employment_premium = EXCLUDED.employment_premium,
            social_insurance_total = EXCLUDED.social_insurance_total,
            income_tax = EXCLUDED.income_tax, resident_tax = EXCLUDED.resident_tax,
            deduction_total = EXCLUDED.deduction_total, net_pay = EXCLUDED.net_pay,
            updated_at = NOW()
        "#
    )
    .bind(data.employee_id)
    .bind(data.year_month)
    .bind(data.work_days)
    .bind(data.total_hours)
    .bind(data.overtime_hours)
    .bind(data.night_hours)
    .bind(data.holiday_hours)
    .bind(data.absence_days)
    .bind(data.base_salary)
    .bind(data.position_allowance)
    .bind(data.housing_allowance)
    .bind(data.commuting_allowance)
    .bind(data.overtime_pay)
    .bind(data.night_pay)
    .bind(data.holiday_pay)
    .bind(data.absence_deduction)
    .bind(data.gross_pay)
    .bind(data.pension_premium)
    .bind(data.health_premium)
    .bind(data.nursing_premium)
    .bind(data.employment_premium)
    .bind(data.social_insurance_total)
    .bind(data.income_tax)
    .bind(data.resident_tax)
    .bind(data.deduction_total)
    .bind(data.net_pay)
    .execute(pool)
    .await?;
    Ok(())
}

/// 既存給与明細の控除欄だけ更新する（ステータス・支給・勤怠は維持）
pub async fn update_payroll_deductions(
    pool: &PgPool,
    id: i64,
    data: &crate::domain::services::payroll_calculator::PayrollData,
) -> Result<u64> {
    let result = sqlx::query(
        r#"
        UPDATE t_payroll SET
            pension_premium = $2,
            health_premium = $3,
            nursing_premium = $4,
            employment_premium = $5,
            social_insurance_total = $6,
            income_tax = $7,
            resident_tax = $8,
            deduction_total = $9,
            net_pay = $10,
            updated_at = NOW()
        WHERE id = $1
        "#
    )
    .bind(id)
    .bind(data.pension_premium)
    .bind(data.health_premium)
    .bind(data.nursing_premium)
    .bind(data.employment_premium)
    .bind(data.social_insurance_total)
    .bind(data.income_tax)
    .bind(data.resident_tax)
    .bind(data.deduction_total)
    .bind(data.net_pay)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 社員件数
pub async fn count_employees(pool: &PgPool) -> Result<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM m_employee WHERE is_active = true"
    )
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

// ── 月次一括計算・確認・振込（2026-07-13追加。P2-3続き:
//    presentation/handlers/payroll.rs 直書きSQLのRepository層移行）──

/// 指定社員・対象月の給与データが既に存在するか
pub async fn payroll_exists(pool: &PgPool, employee_id: i64, year_month: NaiveDate) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_payroll WHERE employee_id = $1 AND year_month = $2)"
    )
    .bind(employee_id)
    .bind(year_month)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// 対象社員・対象月の稼働報告を1件取得する。
/// 案件紐付け（m_client_contract経由、氏名で社員マスタと技術者を突合）を優先し、
/// 無ければ案件非依存の社員自己申告（employee_id直接紐付け、承認済みのみ）にフォールバックする。
pub async fn find_timesheet_for_payroll(
    pool: &PgPool,
    employee_id: i64,
    year_month: NaiveDate,
) -> Result<Option<crate::domain::models::timesheet::MonthlyTimesheet>> {
    let row = sqlx::query_as::<_, crate::domain::models::timesheet::MonthlyTimesheet>(
        r#"
        SELECT * FROM (
            SELECT ts.*, 0 AS source_priority
              FROM t_monthly_timesheet ts
              JOIN m_client_contract cc ON cc.id = ts.client_contract_id
              JOIN m_engineer eng ON eng.id = cc.engineer_id
              JOIN m_employee emp ON emp.id = $1
             WHERE ts.target_month = $2
               AND (
                    (NULLIF(BTRIM(COALESCE(eng.employee_id, '')), '') IS NOT NULL
                     AND eng.employee_id = emp.employee_id)
                 OR (
                      REPLACE(REPLACE(eng.name, '　', ''), ' ', '')
                      = REPLACE(REPLACE(emp.last_name || emp.first_name, '　', ''), ' ', '')
                    )
                 OR (
                      REPLACE(REPLACE(eng.name, '　', ''), ' ', '')
                      = REPLACE(REPLACE(emp.last_name || ' ' || emp.first_name, '　', ''), ' ', '')
                    )
               )
            UNION ALL
            -- 案件非依存の社員自己申告（承認済みのみ給与計算に使う）
            SELECT ts.*, 1 AS source_priority
              FROM t_monthly_timesheet ts
             WHERE ts.employee_id = $1
               AND ts.target_month = $2
               AND ts.status = 'APPROVED'
        ) combined
         ORDER BY
           source_priority,
           CASE WHEN status = 'APPROVED' THEN 0
                WHEN status = 'SENT' THEN 1
                ELSE 2 END,
           id DESC
         LIMIT 1
        "#,
    )
    .bind(employee_id)
    .bind(year_month)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 給与を確認済み(CONFIRMED)にする（DRAFTのもののみ。更新件数を返す）
pub async fn confirm_payroll(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_payroll SET status = 'CONFIRMED', updated_at = NOW() WHERE id = $1 AND status = 'DRAFT'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 給与を振込済(PAID)にする（CONFIRMEDのもののみ。更新件数を返す）
pub async fn mark_payroll_paid(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_payroll SET status = 'PAID', payment_date = CURRENT_DATE, updated_at = NOW() WHERE id = $1 AND status = 'CONFIRMED'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 給与一覧をフィルタ条件（対象月・社員ID）で取得する（ダッシュボードAPI用、最大100件）
pub async fn list_payrolls_filtered(pool: &PgPool, month: &str, employee_id: Option<i64>) -> Result<Vec<PayrollWithEmployee>> {
    let rows = match (month.is_empty(), employee_id) {
        (true, None) => sqlx::query_as::<_, PayrollWithEmployee>(
            r#"SELECT p.id, p.employee_id, p.year_month, p.status, p.gross_pay,
                      p.deduction_total, p.net_pay, e.employee_id AS employee_code,
                      e.last_name, e.first_name
               FROM t_payroll p JOIN m_employee e ON e.id = p.employee_id
               ORDER BY p.year_month DESC, e.employee_id LIMIT 100"#
        ).fetch_all(pool).await,
        (true, Some(eid)) => sqlx::query_as::<_, PayrollWithEmployee>(
            r#"SELECT p.id, p.employee_id, p.year_month, p.status, p.gross_pay,
                      p.deduction_total, p.net_pay, e.employee_id AS employee_code,
                      e.last_name, e.first_name
               FROM t_payroll p JOIN m_employee e ON e.id = p.employee_id
               WHERE p.employee_id = $1
               ORDER BY p.year_month DESC LIMIT 100"#
        ).bind(eid).fetch_all(pool).await,
        (false, None) => sqlx::query_as::<_, PayrollWithEmployee>(
            r#"SELECT p.id, p.employee_id, p.year_month, p.status, p.gross_pay,
                      p.deduction_total, p.net_pay, e.employee_id AS employee_code,
                      e.last_name, e.first_name
               FROM t_payroll p JOIN m_employee e ON e.id = p.employee_id
               WHERE to_char(p.year_month, 'YYYY-MM') = $1
               ORDER BY e.employee_id"#
        ).bind(month).fetch_all(pool).await,
        (false, Some(eid)) => sqlx::query_as::<_, PayrollWithEmployee>(
            r#"SELECT p.id, p.employee_id, p.year_month, p.status, p.gross_pay,
                      p.deduction_total, p.net_pay, e.employee_id AS employee_code,
                      e.last_name, e.first_name
               FROM t_payroll p JOIN m_employee e ON e.id = p.employee_id
               WHERE to_char(p.year_month, 'YYYY-MM') = $1 AND p.employee_id = $2
               ORDER BY e.employee_id"#
        ).bind(month).bind(eid).fetch_all(pool).await,
    }?;
    Ok(rows)
}

/// 社員のフルネーム（姓 名）を取得する
pub async fn find_employee_full_name(pool: &PgPool, employee_id: i64) -> Result<String> {
    let name: String = sqlx::query_scalar(
        "SELECT last_name || ' ' || first_name FROM m_employee WHERE id = $1"
    )
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    Ok(name)
}

/// 給与を確認済み(CONFIRMED)にする（payment_date等を変更しない簡易版。DRAFTのもののみ）
pub async fn confirm_payroll_simple(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("UPDATE t_payroll SET status = 'CONFIRMED' WHERE id = $1 AND status = 'DRAFT'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 給与を振込済(PAID)にする（payment_dateを変更しない簡易版。CONFIRMEDのもののみ）
pub async fn mark_payroll_paid_simple(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("UPDATE t_payroll SET status = 'PAID' WHERE id = $1 AND status = 'CONFIRMED'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 指定した社員コード群の、指定月の給与総支給額(gross_pay)をまとめて取得する。
/// 戻り値のキーは m_employee.employee_id（社員コード文字列）。
/// 対象月の給与レコードが存在しない社員コードはキーごと存在しない（呼び出し側で0円扱いにする）。
pub async fn get_gross_pay_by_employee_codes(
    pool: &PgPool,
    employee_codes: &[String],
    month: NaiveDate,
) -> std::collections::HashMap<String, i32> {
    if employee_codes.is_empty() {
        return std::collections::HashMap::new();
    }
    let rows: Vec<(String, i32)> = sqlx::query_as(
        r#"
        SELECT me.employee_id, p.gross_pay
        FROM m_employee me
        JOIN t_payroll p ON p.employee_id = me.id AND p.year_month = $1
        WHERE me.employee_id = ANY($2)
        "#
    )
    .bind(month)
    .bind(employee_codes)
    .fetch_all(pool)
    .await
    .unwrap_or_else(|e| {
        tracing::error!("get_gross_pay_by_employee_codes: fetch error: {:?}", e);
        vec![]
    });

    rows.into_iter().collect()
}

/// 保険料率取得
pub async fn find_insurance_rate(pool: &PgPool, fiscal_year: i32) -> Result<Option<InsuranceRate>> {
    let row = sqlx::query_as::<_, InsuranceRate>(
        "SELECT * FROM m_insurance_rate WHERE fiscal_year = $1"
    )
    .bind(fiscal_year)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 最新の保険料率取得（フォールバック用）
pub async fn find_latest_insurance_rate(pool: &PgPool) -> Result<Option<InsuranceRate>> {
    let row = sqlx::query_as::<_, InsuranceRate>(
        "SELECT * FROM m_insurance_rate ORDER BY fiscal_year DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 所得税行取得
pub async fn find_withholding_tax_row(pool: &PgPool, fiscal_year: i32, taxable: i32) -> Result<Option<WithholdingTaxRow>> {
    let row = sqlx::query_as::<_, WithholdingTaxRow>(
        "SELECT * FROM m_withholding_tax WHERE fiscal_year = $1 AND salary_from <= $2 AND salary_to > $2 LIMIT 1"
    )
    .bind(fiscal_year)
    .bind(taxable)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 最新所得税年度取得（フォールバック用）
pub async fn find_latest_withholding_tax_year(pool: &PgPool) -> Result<Option<i32>> {
    let year: Option<i32> = sqlx::query_scalar(
        "SELECT fiscal_year FROM m_withholding_tax ORDER BY fiscal_year DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(year)
}

/// 住民税スケジュール取得
pub async fn find_resident_tax_schedule(pool: &PgPool, employee_id: i64, fiscal_year: i32) -> Result<Option<ResidentTaxSchedule>> {
    let row = sqlx::query_as::<_, ResidentTaxSchedule>(
        "SELECT * FROM m_resident_tax_schedule WHERE employee_id = $1 AND fiscal_year = $2"
    )
    .bind(employee_id)
    .bind(fiscal_year)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
