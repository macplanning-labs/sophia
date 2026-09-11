/// domain/models/client_contract.rs — 顧客契約・受注注文書

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use crate::domain::value_objects::SettlementFields;

/// 受注注文書ステータス
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReceivedOrderStatus {
    Registered,
    ReportReceived,
    ReportSent,
    Invoiced,
    Paid,
}

impl ReceivedOrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Registered => "REGISTERED",
            Self::ReportReceived => "REPORT_RECEIVED",
            Self::ReportSent => "REPORT_SENT",
            Self::Invoiced => "INVOICED",
            Self::Paid => "PAID",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Registered => "受注登録",
            Self::ReportReceived => "勤怠受領",
            Self::ReportSent => "報告書送付",
            Self::Invoiced => "請求書処理",
            Self::Paid => "入金済",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "REGISTERED" => Self::Registered,
            "REPORT_RECEIVED" => Self::ReportReceived,
            "REPORT_SENT" => Self::ReportSent,
            "INVOICED" => Self::Invoiced,
            "PAID" => Self::Paid,
            _ => Self::Registered,
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Registered => "bg-secondary",
            Self::ReportReceived => "bg-warning text-dark",
            Self::ReportSent => "bg-info",
            Self::Invoiced => "bg-primary",
            Self::Paid => "bg-success",
        }
    }
}

/// 顧客契約（受注マスタ）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ClientContract {
    pub id: i64,
    pub project_id: String,
    pub engineer_id: i64,
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
    pub mid_month_rule: String,
    pub billing_timing: String,
    pub payment_terms: String,
    pub report_deadline_days_before: i32,
    pub currency: String,
    pub remarks: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SettlementFields for ClientContract {
    fn lower_limit_hours(&self) -> Decimal { self.lower_limit_hours }
    fn upper_limit_hours(&self) -> Decimal { self.upper_limit_hours }
    fn fixed_hours(&self) -> Option<Decimal> { self.fixed_hours }
    fn deduction_rate(&self) -> i32 { self.deduction_rate }
    fn overtime_rate(&self) -> i32 { self.overtime_rate }
}

/// 受注注文書
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ReceivedOrder {
    pub id: i64,
    pub uuid: uuid::Uuid,
    pub received_order_no: String,
    pub client_id: i64,
    pub engineer_id: Option<i64>,
    pub client_contract_id: Option<i64>,
    pub client_order_number: String,
    pub target_month: NaiveDate,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
    pub project_name: String,
    pub payment_condition: String,
    pub status: String,
    pub order_file: String,
    pub is_recurring: bool,
    pub parent_order_id: Option<i64>,
    pub order_date: NaiveDate,
    pub remarks: String,
    pub report_to_email: String,
    pub report_cc_emails: String,
    pub invoice_to_email: String,
    pub invoice_cc_emails: String,
    pub invoice_confirmed: bool,
    pub invoice_confirmed_at: Option<DateTime<Utc>>,
    pub payment_confirmed: bool,
    pub payment_confirmed_at: Option<DateTime<Utc>>,
    pub report_sent_to_client: bool,
    pub report_sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 受注注文書明細（精算条件スナップショット含む）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ReceivedOrderItem {
    pub id: i64,
    pub order_id: i64,
    pub client_contract_id: Option<i64>,
    pub engineer_name: String,
    pub unit_price: i32,
    pub man_month: Decimal,
    pub actual_hours: Decimal,
    pub adjustment: i32,
    pub amount: i32,
    pub settlement_type: String,
    pub base_rate: i32,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub effort: Decimal,
    pub mid_month_rule: String,
}

impl SettlementFields for ReceivedOrderItem {
    fn lower_limit_hours(&self) -> Decimal { self.lower_limit_hours }
    fn upper_limit_hours(&self) -> Decimal { self.upper_limit_hours }
    fn fixed_hours(&self) -> Option<Decimal> { self.fixed_hours }
    fn deduction_rate(&self) -> i32 { self.deduction_rate }
    fn overtime_rate(&self) -> i32 { self.overtime_rate }
}

/// 受注注文書作成フォーム
#[derive(Debug, Deserialize)]
pub struct ReceivedOrderForm {
    pub client_id: i64,
    pub client_contract_id: Option<i64>,
    pub target_month: NaiveDate,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
    pub project_name: Option<String>,
    pub client_order_number: Option<String>,
}

/// 顧客契約 作成・編集フォーム
#[derive(Debug, Deserialize)]
pub struct ClientContractForm {
    pub project_id: String,
    pub engineer_id: i64,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub settlement_type: String,
    pub base_rate: i32,
    pub effort: Decimal,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub mid_month_rule: Option<String>,
    pub billing_timing: Option<String>,
    pub payment_terms: Option<String>,
    pub report_deadline_days_before: Option<i32>,
    pub currency: Option<String>,
    pub remarks: Option<String>,
    pub is_active: Option<String>, // checkbox: "on" or absent
}

/// 顧客契約 + 案件名・エンジニア名（一覧・詳細表示用）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ClientContractWithNames {
    pub id: i64,
    pub project_id: String,
    pub engineer_id: i64,
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
    pub mid_month_rule: String,
    pub billing_timing: String,
    pub payment_terms: String,
    pub report_deadline_days_before: i32,
    pub currency: String,
    pub remarks: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    // JOIN結果
    pub project_name: String,
    pub client_name: String,
    pub engineer_name: String,
}
