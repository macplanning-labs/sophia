/// domain/models/partner_contract.rs — パートナー契約・注文書・支払通知

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use crate::domain::value_objects::SettlementFields;

/// 発注注文書ステータス
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PurchaseOrderStatus {
    Draft,
    Sent,
    Accepted,
    ReportReceived,
    NoticeCreated,
    NoticeConfirmed,
    Paid,
}

impl PurchaseOrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Sent => "SENT",
            Self::Accepted => "ACCEPTED",
            Self::ReportReceived => "REPORT_RECEIVED",
            Self::NoticeCreated => "NOTICE_CREATED",
            Self::NoticeConfirmed => "NOTICE_CONFIRMED",
            Self::Paid => "PAID",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Draft => "起票",
            Self::Sent => "送付済",
            Self::Accepted => "受諾済",
            Self::ReportReceived => "報告書受領",
            Self::NoticeCreated => "支払通知作成",
            Self::NoticeConfirmed => "支払通知受諾",
            Self::Paid => "支払済",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "DRAFT" => Self::Draft,
            "SENT" => Self::Sent,
            "ACCEPTED" => Self::Accepted,
            "REPORT_RECEIVED" => Self::ReportReceived,
            "NOTICE_CREATED" => Self::NoticeCreated,
            "NOTICE_CONFIRMED" => Self::NoticeConfirmed,
            "PAID" => Self::Paid,
            _ => Self::Draft,
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Draft => "bg-secondary",
            Self::Sent => "bg-primary",
            Self::Accepted => "bg-info",
            Self::ReportReceived => "bg-warning text-dark",
            Self::NoticeCreated => "bg-info",
            Self::NoticeConfirmed => "bg-success",
            Self::Paid => "bg-dark",
        }
    }
}

/// パートナー契約（発注マスタ）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PartnerContract {
    pub id: i64,
    pub project_id: String,
    pub engineer_id: i64,
    pub partner_id: String,
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
    // 注文書テンプレート情報
    #[sqlx(rename = "甲_責任者")]
    pub kou_responsible: String,
    #[sqlx(rename = "甲_担当者")]
    pub kou_contact: String,
    #[sqlx(rename = "乙_責任者")]
    pub otsu_responsible: String,
    #[sqlx(rename = "乙_担当者")]
    pub otsu_contact: String,
    #[sqlx(rename = "作業責任者")]
    pub work_responsible: String,
    pub workplace_id: Option<i64>,
    pub work_location: String,
    pub deliverable_text: String,
    pub payment_condition: String,
    pub contract_items: String,
    pub currency: String,
    pub reminder_days_before: i32,
    pub alert_days_after: i32,
    pub remarks: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SettlementFields for PartnerContract {
    fn lower_limit_hours(&self) -> Decimal { self.lower_limit_hours }
    fn upper_limit_hours(&self) -> Decimal { self.upper_limit_hours }
    fn fixed_hours(&self) -> Option<Decimal> { self.fixed_hours }
    fn deduction_rate(&self) -> i32 { self.deduction_rate }
    fn overtime_rate(&self) -> i32 { self.overtime_rate }
}

/// 発注注文書
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PurchaseOrder {
    pub order_id: String,
    pub uuid: uuid::Uuid,
    pub status: String,
    pub partner_id: String,
    pub project_id: String,
    pub engineer_id: Option<i64>,
    pub partner_contract_id: Option<i64>,
    pub order_date: NaiveDate,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
    pub workplace_id: Option<i64>,
    pub deliverable_text: String,
    pub payment_condition: String,
    pub contract_items: String,
    pub work_location: String,
    #[sqlx(rename = "甲_責任者")]
    #[serde(rename = "甲_責任者")]
    pub kou_manager: String,
    #[sqlx(rename = "甲_担当者")]
    #[serde(rename = "甲_担当者")]
    pub kou_person: String,
    #[sqlx(rename = "乙_責任者")]
    #[serde(rename = "乙_責任者")]
    pub otsu_manager: String,
    #[sqlx(rename = "乙_担当者")]
    #[serde(rename = "乙_担当者")]
    pub otsu_person: String,
    #[sqlx(rename = "作業責任者")]
    #[serde(rename = "作業責任者")]
    pub work_manager: String,
    pub remarks: String,
    pub partner_accepted_at: Option<DateTime<Utc>>,
    pub token_issued_at: Option<DateTime<Utc>>,
    pub document_hash: String,
    pub order_pdf: String,
    pub acceptance_pdf: String,
    pub drive_file_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PurchaseOrder {
    /// トークンが有効かチェック
    pub fn is_token_valid(&self) -> bool {
        if self.partner_accepted_at.is_some() {
            return false; // 既に承諾済み
        }
        match self.token_issued_at {
            Some(issued) => {
                let deadline = issued + chrono::Duration::hours(24);
                Utc::now() < deadline
            }
            None => true, // 旧データは後方互換で期限なし
        }
    }
}

/// 発注注文書明細（スナップショット）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PurchaseOrderItem {
    pub id: i64,
    pub order_id: String,
    pub partner_contract_id: i64,
    pub base_fee: i32,
    pub effort: Decimal,
    pub actual_hours: Decimal,
    pub settlement_type: String,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    /// DBカラム名はmigration 047で`price`→`amount`に変更したが、
    /// 公開JSON APIのキー名（フロントエンド契約）は変更しない方針のため`price`のまま維持する
    #[serde(rename = "price")]
    pub amount: i32,
    pub tax_rate: Decimal,
}

impl SettlementFields for PurchaseOrderItem {
    fn lower_limit_hours(&self) -> Decimal { self.lower_limit_hours }
    fn upper_limit_hours(&self) -> Decimal { self.upper_limit_hours }
    fn fixed_hours(&self) -> Option<Decimal> { self.fixed_hours }
    fn deduction_rate(&self) -> i32 { self.deduction_rate }
    fn overtime_rate(&self) -> i32 { self.overtime_rate }
}

/// 上司承認ステータス（支払通知書）
///
/// 遷移ルール1箇所定義（開発標準§2.2 #6）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ApprovalStatus {
    /// 承認不要（基準金額未満）
    None,
    /// 上司承認待ち
    PendingApproval,
    /// 承認済み → メール送信可能
    Approved,
    /// 差戻し → 修正後再送
    Rejected,
}

impl ApprovalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "PENDING_APPROVAL" => Self::PendingApproval,
            "APPROVED" => Self::Approved,
            "REJECTED" => Self::Rejected,
            _ => Self::None,
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::None => "承認不要",
            Self::PendingApproval => "承認待ち",
            Self::Approved => "承認済",
            Self::Rejected => "差戻し",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::None => "bg-secondary",
            Self::PendingApproval => "bg-warning text-dark",
            Self::Approved => "bg-success",
            Self::Rejected => "bg-danger",
        }
    }

    /// 遷移ルール
    pub fn can_transition_to(&self, next: &Self) -> bool {
        matches!((self, next),
            (Self::PendingApproval, Self::Approved) |
            (Self::PendingApproval, Self::Rejected) |
            (Self::Rejected, Self::PendingApproval)
        )
    }

    /// メール送信可能か
    pub fn can_send_mail(&self) -> bool {
        matches!(self, Self::None | Self::Approved)
    }
}

/// 支払通知書
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PaymentNotice {
    pub notice_id: String,
    pub uuid: uuid::Uuid,
    pub purchase_order_id: String,
    pub partner_id: String,
    pub target_month: NaiveDate,
    pub notice_date: NaiveDate,
    pub payment_due_date: Option<NaiveDate>,
    pub subtotal: i32,
    pub tax_amount: i32,
    pub total: i32,
    pub partner_accepted_at: Option<DateTime<Utc>>,
    pub notice_pdf: String,
    pub remarks: String,
    // 上司承認フロー
    pub approval_status: Option<String>,
    pub approval_requested_at: Option<DateTime<Utc>>,
    pub approval_requested_by_id: Option<i64>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by_id: Option<i64>,
    pub mail_sent_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub document_hash: String,
    pub drive_file_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 支払通知書明細
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PaymentNoticeItem {
    pub id: i64,
    pub notice_id: String,
    pub partner_contract_id: i64,
    pub actual_hours: Decimal,
    pub base_fee: i32,
    pub effort: Decimal,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub adjustment: i32,
    pub amount: i32,
    pub tax_rate: Decimal,
}

/// 注文書作成フォーム
#[derive(Debug, Deserialize)]
pub struct PurchaseOrderForm {
    pub partner_id: String,
    pub project_id: String,
    pub partner_contract_id: Option<i64>,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
}
