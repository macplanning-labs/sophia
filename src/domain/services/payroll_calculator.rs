/// domain/services/payroll_calculator.rs — 給与一括計算サービス
///
/// 社員マスタ + 稼働報告から月次給与を計算する。
/// 支給 → 社会保険 → 所得税 → 住民税 → 差引支給額の順に計算。

use anyhow::Result;
use chrono::{NaiveDate, Datelike};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use sqlx::PgPool;

use crate::domain::models::payroll::*;
use crate::domain::models::timesheet::MonthlyTimesheet;
use crate::infrastructure::repositories::payroll_repo;

/// 給与計算サービス
pub struct PayrollCalculator<'a> {
    pool: &'a PgPool,
}

impl<'a> PayrollCalculator<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// 単一社員の給与を計算する
    pub async fn calculate(
        &self,
        employee: &Employee,
        year_month: NaiveDate,
        timesheet: Option<&MonthlyTimesheet>,
    ) -> Result<PayrollData> {
        let mut data = PayrollData::new(employee, year_month);

        // 勤怠取込
        self.set_attendance(&mut data, timesheet);

        // 支給計算
        self.calc_pay(&mut data, employee);

        // 社会保険料
        self.calc_social_insurance(&mut data, employee, year_month).await?;

        // 所得税
        self.calc_income_tax(&mut data, employee, year_month).await?;

        // 住民税
        self.calc_resident_tax(&mut data, employee, year_month).await?;

        // 合計
        self.calc_totals(&mut data);

        Ok(data)
    }

    /// 既存明細の支給・勤怠を維持したまま、控除（社保・所得税・住民税）と差引だけ再計算する。
    /// インポートで控除が空の明細を埋める用途。
    pub async fn recalculate_deductions(
        &self,
        payroll: &Payroll,
        employee: &Employee,
    ) -> Result<PayrollData> {
        let mut data = PayrollData {
            employee_id: payroll.employee_id,
            year_month: payroll.year_month,
            work_days: payroll.work_days,
            total_hours: payroll.total_hours,
            overtime_hours: payroll.overtime_hours,
            night_hours: payroll.night_hours,
            holiday_hours: payroll.holiday_hours,
            absence_days: payroll.absence_days,
            base_salary: payroll.base_salary,
            position_allowance: payroll.position_allowance,
            housing_allowance: payroll.housing_allowance,
            commuting_allowance: payroll.commuting_allowance,
            overtime_pay: payroll.overtime_pay,
            night_pay: payroll.night_pay,
            holiday_pay: payroll.holiday_pay,
            absence_deduction: payroll.absence_deduction,
            gross_pay: payroll.gross_pay,
            ..Default::default()
        };

        self.calc_social_insurance(&mut data, employee, payroll.year_month).await?;
        self.calc_income_tax(&mut data, employee, payroll.year_month).await?;
        self.calc_resident_tax(&mut data, employee, payroll.year_month).await?;
        self.calc_totals(&mut data);

        Ok(data)
    }

    fn set_attendance(&self, data: &mut PayrollData, timesheet: Option<&MonthlyTimesheet>) {
        if let Some(ts) = timesheet {
            data.work_days = ts.work_days;
            data.total_hours = ts.total_hours;
            data.overtime_hours = ts.overtime_hours;
            data.night_hours = ts.night_hours;
            data.holiday_hours = ts.holiday_hours;
        }
    }

    fn calc_pay(&self, data: &mut PayrollData, emp: &Employee) {
        data.base_salary = emp.base_salary;
        data.position_allowance = emp.position_allowance;
        data.housing_allowance = emp.housing_allowance;
        data.commuting_allowance = emp.commuting_allowance;

        let hourly = emp.hourly_rate();

        // 残業手当 = 時間単価 × 1.25 × 残業時間
        data.overtime_pay = (hourly * Decimal::new(125, 2) * data.overtime_hours)
            .to_i32().unwrap_or(0);

        // 深夜手当 = 時間単価 × 1.50 × 深夜時間
        data.night_pay = (hourly * Decimal::new(150, 2) * data.night_hours)
            .to_i32().unwrap_or(0);

        // 休出手当 = 時間単価 × 1.35 × 休出時間
        data.holiday_pay = (hourly * Decimal::new(135, 2) * data.holiday_hours)
            .to_i32().unwrap_or(0);

        // 欠勤控除 = 時間単価 × 標準月間時間 ÷ 標準出勤日数 × 欠勤日数
        data.absence_deduction = 0; // 欠勤日数は別途入力が必要

        data.gross_pay = data.base_salary
            + data.position_allowance
            + data.housing_allowance
            + data.commuting_allowance
            + data.overtime_pay
            + data.night_pay
            + data.holiday_pay
            - data.absence_deduction;
    }

    async fn calc_social_insurance(
        &self,
        data: &mut PayrollData,
        emp: &Employee,
        year_month: NaiveDate,
    ) -> Result<()> {
        let fiscal_year = if year_month.month() >= 4 {
            year_month.year()
        } else {
            year_month.year() - 1
        };

        // 対象年度 → なければ直近年度にフォールバック
        let rate = payroll_repo::find_insurance_rate(self.pool, fiscal_year).await?;
        let rate = match rate {
            Some(r) => Some(r),
            None => {
                tracing::info!("m_insurance_rate: fiscal_year={} not found, falling back to latest", fiscal_year);
                payroll_repo::find_latest_insurance_rate(self.pool).await?
            }
        };

        if let Some(rate) = rate {
            let sr = Decimal::from(emp.standard_remuneration);

            // 厚生年金 = half_round(標準報酬月額 × 料率 ÷ 100)
            if emp.pension_enrolled {
                data.pension_premium = Self::half_round(sr, rate.pension_rate);
            }

            // 健康保険
            if emp.health_enrolled {
                data.health_premium = Self::half_round(sr, rate.health_rate);
            }

            // 介護保険（40歳以上のみ）
            if emp.nursing_enrolled {
                data.nursing_premium = Self::half_round(sr, rate.nursing_rate);
            }

            // 雇用保険 = 支給合計 × 料率 ÷ 100（折半なし）
            if emp.employment_enrolled {
                data.employment_premium = (Decimal::from(data.gross_pay)
                    * rate.employment_rate_employee / Decimal::from(100))
                    .to_i32().unwrap_or(0);
            }
        }

        data.social_insurance_total = data.pension_premium
            + data.health_premium
            + data.nursing_premium
            + data.employment_premium;

        Ok(())
    }

    /// 社保料の折半端数処理
    /// 標準報酬月額 × 料率% ÷ 2 で、50銭超は切上げ・50銭以下は切捨て
    fn half_round(standard_remuneration: Decimal, rate: Decimal) -> i32 {
        let total = standard_remuneration * rate / Decimal::from(100);
        let half = total / Decimal::from(2);
        // 50銭超切上 / 50銭以下切捨
        // Decimal の丸め: ROUND_HALF_DOWN（0.5以下切捨、0.5超切上）
        half.round_dp_with_strategy(0, rust_decimal::RoundingStrategy::MidpointTowardZero)
            .to_i32().unwrap_or(0)
    }

    /// 50円単位丸め（通勤手当等）
    fn round50(value: i32) -> i32 {
        ((value + 25) / 50) * 50
    }


    async fn calc_income_tax(
        &self,
        data: &mut PayrollData,
        emp: &Employee,
        year_month: NaiveDate,
    ) -> Result<()> {
        if emp.is_tax_exempt {
            data.income_tax = 0;
            return Ok(());
        }

        // 課税対象額 = 支給合計 − 通勤手当（非課税）− 社会保険料合計
        let taxable = data.gross_pay - data.commuting_allowance - data.social_insurance_total;
        if taxable <= 0 {
            data.income_tax = 0;
            return Ok(());
        }

        let fiscal_year = year_month.year();

        // 対象年度 → なければ直近年度にフォールバック
        let tax_row = payroll_repo::find_withholding_tax_row(self.pool, fiscal_year, taxable).await?;
        let tax_row = match tax_row {
            Some(r) => Some(r),
            None => {
                tracing::info!("m_withholding_tax: fiscal_year={} not found, falling back to latest", fiscal_year);
                if let Some(fy) = payroll_repo::find_latest_withholding_tax_year(self.pool).await? {
                    payroll_repo::find_withholding_tax_row(self.pool, fy, taxable).await?
                } else {
                    None
                }
            }
        };

        if let Some(row) = tax_row {
            data.income_tax = row.tax_for_dependents(emp.dependents_count);
        }

        Ok(())
    }

    async fn calc_resident_tax(
        &self,
        data: &mut PayrollData,
        emp: &Employee,
        year_month: NaiveDate,
    ) -> Result<()> {
        // 住民税は6月〜翌5月の特別徴収スケジュール
        let fiscal_year = if year_month.month() >= 6 {
            year_month.year()
        } else {
            year_month.year() - 1
        };

        let schedule = payroll_repo::find_resident_tax_schedule(self.pool, emp.id, fiscal_year).await?;

        if let Some(sched) = schedule {
            data.resident_tax = sched.tax_for_month(year_month.month());
        }

        Ok(())
    }

    fn calc_totals(&self, data: &mut PayrollData) {
        data.deduction_total = data.social_insurance_total
            + data.income_tax
            + data.resident_tax;
        data.net_pay = data.gross_pay - data.deduction_total;
    }
}

/// 給与計算中間データ
#[derive(Debug, Default)]
pub struct PayrollData {
    pub employee_id: i64,
    pub year_month: NaiveDate,
    // 勤怠
    pub work_days: i32,
    pub total_hours: Decimal,
    pub overtime_hours: Decimal,
    pub night_hours: Decimal,
    pub holiday_hours: Decimal,
    pub absence_days: i32,
    // 支給
    pub base_salary: i32,
    pub position_allowance: i32,
    pub housing_allowance: i32,
    pub commuting_allowance: i32,
    pub overtime_pay: i32,
    pub night_pay: i32,
    pub holiday_pay: i32,
    pub absence_deduction: i32,
    pub gross_pay: i32,
    // 控除
    pub pension_premium: i32,
    pub health_premium: i32,
    pub nursing_premium: i32,
    pub employment_premium: i32,
    pub social_insurance_total: i32,
    pub income_tax: i32,
    pub resident_tax: i32,
    pub deduction_total: i32,
    pub net_pay: i32,
}

impl PayrollData {
    pub fn new(emp: &Employee, year_month: NaiveDate) -> Self {
        Self {
            employee_id: emp.id,
            year_month,
            ..Default::default()
        }
    }
}
