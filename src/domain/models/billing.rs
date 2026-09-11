/// domain/models/billing.rs — 請求書・入金記録

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 請求書（t_billing_invoice に対応）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct BillingInvoice {
    pub id: i64,
    pub uuid: uuid::Uuid,
    #[sqlx(rename = "invoice_no")]
    #[serde(rename = "invoice_id")]
    pub invoice_no: String,
    pub client_id: i64,
    pub received_order_id: Option<i64>,
    pub target_month: NaiveDate,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
    pub issue_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub subtotal: i32,
    pub tax_amount: i32,
    pub total: i32,
    pub department: String,
    pub subject: String,
    pub registration_no: String,
    pub edi_id: Option<i64>,
    pub edi_invoice_no: String,
    pub source: String,
    pub status: String,
    pub client_accepted_at: Option<DateTime<Utc>>,
    pub document_hash: String,
    pub invoice_pdf: String,
    pub drive_file_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub approved_by_id: Option<i64>,
    pub approved_at: Option<DateTime<Utc>>,
    pub sent_at: Option<DateTime<Utc>>,
    pub sent_subject: String,
    pub sent_body: String,
}

/// 請求明細（t_billing_invoice_item に対応）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct BillingItem {
    pub id: i64,
    pub invoice_id: i64,
    pub engineer_id: i64,
    pub description: String,
    pub quantity: Decimal,
    pub unit_price: i32,
    pub amount: i32,
    pub settlement_type: String,
    pub lower_limit: Decimal,
    pub upper_limit: Decimal,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub tax_rate: Decimal,
    pub created_at: DateTime<Utc>,
}

/// 入金記録
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PaymentRecord {
    pub id: i64,
    pub invoice_id: i64,
    pub payment_date: NaiveDate,
    pub amount: i32,
    pub method: String,
    pub reference: String,
    pub confirmed_by_id: Option<i64>,
    pub confirmed_at: DateTime<Utc>,
}

/// 請求書作成フォーム
#[derive(Debug, Deserialize)]
pub struct InvoiceForm {
    pub client_id: i64,
    pub received_order_id: Option<i64>,
    pub issue_date: NaiveDate,
    #[serde(default, deserialize_with = "crate::domain::serde_helpers::deserialize_optional_date")]
    pub due_date: Option<NaiveDate>,
    pub subject: Option<String>,
    pub notes: Option<String>,
}

/// 入金登録フォーム
#[derive(Debug, Deserialize)]
pub struct PaymentRecordForm {
    pub invoice_id: i64,
    pub payment_date: NaiveDate,
    pub amount: i32,
    pub method: Option<String>,
    pub reference: Option<String>,
}
