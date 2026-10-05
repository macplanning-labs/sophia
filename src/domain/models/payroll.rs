/// domain/models/payroll.rs — 給与計算関連エンティティ
///
/// Employee（社員マスタ）, Payroll（給与計算結果）,
/// InsuranceRate（保険料率）, WithholdingTaxRow（源泉徴収税額表）,
/// ResidentTaxSchedule（住民税スケジュール）

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 給与ステータス
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PayrollStatus {
    Draft,
    Confirmed,
    Paid,
}

impl PayrollStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Confirmed => "CONFIRMED",
            Self::Paid => "PAID",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Draft => "下書き",
            Self::Confirmed => "確認済",
            Self::Paid => "振込済",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "DRAFT" => Self::Draft,
            "CONFIRMED" => Self::Confirmed,
            "PAID" => Self::Paid,
            _ => Self::Draft,
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Draft => "bg-secondary",
            Self::Confirmed => "bg-primary",
            Self::Paid => "bg-success",
        }
    }
}

/// 社員マスタ
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Employee {
    pub id: i64,
    pub employee_id: String,
    pub last_name: String,
    pub first_name: String,
    pub last_name_kana: String,
    pub first_name_kana: String,
    pub employment_type: String,
    pub birth_date: Option<NaiveDate>,
    pub hire_date: Option<NaiveDate>,
    pub email: String,
    pub base_salary: i32,
    pub position_allowance: i32,
    pub housing_allowance: i32,
    pub commuting_allowance: i32,
    pub standard_monthly_hours: Decimal,
    pub standard_remuneration: i32,
    pub insurance_start_date: Option<NaiveDate>,
    pub dependents_count: i32,
    pub is_tax_exempt: bool,
    pub pension_enrolled: bool,
    pub health_enrolled: bool,
    pub nursing_enrolled: bool,
    pub employment_enrolled: bool,
    pub is_active: bool,
    pub weekly_prescribed_days: Option<i16>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Employee {
    /// 氏名（姓 名）
    pub fn full_name(&self) -> String {
        format!("{} {}", self.last_name, self.first_name)
    }

    /// フリガナ（姓 名）
    pub fn full_name_kana(&self) -> String {
        format!("{} {}", self.last_name_kana, self.first_name_kana)
    }

    /// 時間単価（基本給 ÷ 標準月間時間）
    pub fn hourly_rate(&self) -> Decimal {
        if self.standard_monthly_hours == Decimal::ZERO {
            return Decimal::ZERO;
        }
        Decimal::from(self.base_salary) / self.standard_monthly_hours
    }
}

/// 社員振込先
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct EmployeeBankAccount {
    pub id: i64,
    pub employee_id: i64,
    pub bank_name: String,
    pub bank_code: String,
    pub branch_name: String,
    pub branch_code: String,
    pub account_type: String,
    pub account_number: String,
    pub account_holder_kana: String,
}

/// 保険料率マスタ
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct InsuranceRate {
    pub id: i64,
    pub fiscal_year: i32,
    pub pension_rate: Decimal,
    pub health_rate: Decimal,
    pub nursing_rate: Decimal,
    pub employment_rate_employee: Decimal,
}

/// 源泉徴収税額表
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WithholdingTaxRow {
    pub id: i64,
    pub fiscal_year: i32,
    pub salary_from: i32,
    pub salary_to: i32,
    pub tax_dep_0: i32,
    pub tax_dep_1: i32,
    pub tax_dep_2: i32,
    pub tax_dep_3: i32,
    pub tax_dep_4: i32,
    pub tax_dep_5: i32,
    pub tax_dep_6: i32,
    pub tax_dep_7: i32,
}

impl WithholdingTaxRow {
    /// 扶養人数に応じた税額を返す
    pub fn tax_for_dependents(&self, deps: i32) -> i32 {
        match deps {
            0 => self.tax_dep_0,
            1 => self.tax_dep_1,
            2 => self.tax_dep_2,
            3 => self.tax_dep_3,
            4 => self.tax_dep_4,
            5 => self.tax_dep_5,
            6 => self.tax_dep_6,
            _ => self.tax_dep_7,
        }
    }
}

/// 住民税スケジュール
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ResidentTaxSchedule {
    pub id: i64,
    pub employee_id: i64,
    pub fiscal_year: i32,
    pub municipality: String,
    pub month_06: i32,
    pub month_07: i32,
    pub month_08: i32,
    pub month_09: i32,
    pub month_10: i32,
    pub month_11: i32,
    pub month_12: i32,
    pub month_01: i32,
    pub month_02: i32,
    pub month_03: i32,
    pub month_04: i32,
    pub month_05: i32,
}

impl ResidentTaxSchedule {
    /// 指定月の住民税額を返す
    pub fn tax_for_month(&self, month: u32) -> i32 {
        match month {
            1 => self.month_01,
            2 => self.month_02,
            3 => self.month_03,
            4 => self.month_04,
            5 => self.month_05,
            6 => self.month_06,
            7 => self.month_07,
            8 => self.month_08,
            9 => self.month_09,
            10 => self.month_10,
            11 => self.month_11,
            12 => self.month_12,
            _ => 0,
        }
    }
}

/// 給与計算結果
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Payroll {
    pub id: i64,
    pub employee_id: i64,
    pub year_month: NaiveDate,
    pub timesheet_id: Option<i64>,
    pub status: String,
    pub payment_date: Option<NaiveDate>,
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
    // 差引
    pub net_pay: i32,
    // 有給休暇（確定時点のスナップショット）
    pub paid_leave_used_days: Decimal,
    pub paid_leave_balance_days: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 有給休暇 付与バッチ（法定基準日ごとに1行。時効2年は expire_date で管理）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PaidLeaveGrant {
    pub id: i64,
    pub employee_id: i64,
    pub grant_date: NaiveDate,
    pub granted_days: Decimal,
    pub expire_date: NaiveDate,
    pub remaining_days: Decimal,
    pub created_at: DateTime<Utc>,
}

/// 有給休暇 取得履歴（1申請=1行）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PaidLeaveUsage {
    pub id: i64,
    pub employee_id: i64,
    pub used_date: NaiveDate,
    pub days_used: Decimal,
    pub year_month: NaiveDate,
    pub created_at: DateTime<Utc>,
}

/// 給与 + 社員名（一覧表示用）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PayrollWithEmployee {
    pub id: i64,
    pub employee_id: i64,
    pub year_month: NaiveDate,
    pub status: String,
    pub gross_pay: i32,
    pub deduction_total: i32,
    pub net_pay: i32,
    // JOIN
    pub employee_code: String,
    pub last_name: String,
    pub first_name: String,
}

/// 社員登録・編集フォーム
#[derive(Debug, Deserialize)]
pub struct EmployeeForm {
    pub employee_id: String,
    pub last_name: String,
    pub first_name: String,
    pub last_name_kana: Option<String>,
    pub first_name_kana: Option<String>,
    pub employment_type: Option<String>,
    #[serde(default, deserialize_with = "crate::domain::serde_helpers::deserialize_optional_date")]
    pub birth_date: Option<NaiveDate>,
    #[serde(default, deserialize_with = "crate::domain::serde_helpers::deserialize_optional_date")]
    pub hire_date: Option<NaiveDate>,
    pub email: Option<String>,
    pub base_salary: Option<i32>,
    pub position_allowance: Option<i32>,
    pub housing_allowance: Option<i32>,
    pub commuting_allowance: Option<i32>,
    pub standard_monthly_hours: Option<Decimal>,
    pub standard_remuneration: Option<i32>,
    pub dependents_count: Option<i32>,
}

/// 給与一括計算フォーム
#[derive(Debug, Deserialize)]
pub struct PayrollCalcForm {
    pub year_month: NaiveDate,
}

