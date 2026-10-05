/// domain/models/task.rs — 月次タスク管理

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// タスク種別（10種）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskType {
    OrderCreate,
    OrderApprove,
    PaymentNotice,
    ReceivedOrder,
    ReportUpload,
    ReportApprove,
    ReportSend,
    InvoiceCreate,
    InvoiceSend,
    PaymentConfirm,
}

impl TaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OrderCreate => "ORDER_CREATE",
            Self::OrderApprove => "ORDER_APPROVE",
            Self::PaymentNotice => "PAYMENT_NOTICE",
            Self::ReceivedOrder => "RECEIVED_ORDER",
            Self::ReportUpload => "REPORT_UPLOAD",
            Self::ReportApprove => "REPORT_APPROVE",
            Self::ReportSend => "REPORT_SEND",
            Self::InvoiceCreate => "INVOICE_CREATE",
            Self::InvoiceSend => "INVOICE_SEND",
            Self::PaymentConfirm => "PAYMENT_CONFIRM",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::OrderCreate => "注文書作成",
            Self::OrderApprove => "注文書承認",
            Self::PaymentNotice => "支払通知作成",
            Self::ReceivedOrder => "受注登録",
            Self::ReportUpload => "稼働報告UP",
            Self::ReportApprove => "稼働報告承認",
            Self::ReportSend => "報告書送付",
            Self::InvoiceCreate => "請求書作成",
            Self::InvoiceSend => "請求書送付",
            Self::PaymentConfirm => "入金確認",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "ORDER_CREATE" => Self::OrderCreate,
            "ORDER_APPROVE" => Self::OrderApprove,
            "PAYMENT_NOTICE" => Self::PaymentNotice,
            "RECEIVED_ORDER" => Self::ReceivedOrder,
            "REPORT_UPLOAD" => Self::ReportUpload,
            "REPORT_APPROVE" => Self::ReportApprove,
            "REPORT_SEND" => Self::ReportSend,
            "INVOICE_CREATE" => Self::InvoiceCreate,
            "INVOICE_SEND" => Self::InvoiceSend,
            "PAYMENT_CONFIRM" => Self::PaymentConfirm,
            _ => Self::OrderCreate,
        }
    }

    /// 全種別を返す
    pub fn all() -> Vec<Self> {
        vec![
            Self::OrderCreate,
            Self::OrderApprove,
            Self::PaymentNotice,
            Self::ReceivedOrder,
            Self::ReportUpload,
            Self::ReportApprove,
            Self::ReportSend,
            Self::InvoiceCreate,
            Self::InvoiceSend,
            Self::PaymentConfirm,
        ]
    }
}

/// タスクステータス
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
    Overdue,
    Skipped,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::InProgress => "IN_PROGRESS",
            Self::Done => "DONE",
            Self::Overdue => "OVERDUE",
            Self::Skipped => "SKIPPED",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Pending => "未着手",
            Self::InProgress => "進行中",
            Self::Done => "完了",
            Self::Overdue => "期限超過",
            Self::Skipped => "スキップ",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "PENDING" => Self::Pending,
            "IN_PROGRESS" => Self::InProgress,
            "DONE" => Self::Done,
            "OVERDUE" => Self::Overdue,
            "SKIPPED" => Self::Skipped,
            _ => Self::Pending,
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Pending => "bg-secondary",
            Self::InProgress => "bg-primary",
            Self::Done => "bg-success",
            Self::Overdue => "bg-danger",
            Self::Skipped => "bg-dark",
        }
    }
}

/// 月次タスク
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MonthlyTask {
    pub id: i64,
    pub project_id: String,
    pub engineer_id: Option<i64>,
    pub work_month: NaiveDate,
    pub task_type: String,
    pub responsible: String,
    pub deadline: NaiveDate,
    pub status: String,
    pub completed_at: Option<DateTime<Utc>>,
    pub note: String,
    pub reminder_sent: bool,
    pub alert_sent: bool,
    pub deadline_notified: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MonthlyTask {
    /// 期限超過かチェック
    pub fn is_overdue(&self) -> bool {
        let status = TaskStatus::from_str(&self.status);
        if status == TaskStatus::Done || status == TaskStatus::Skipped {
            return false;
        }
        self.deadline < chrono::Utc::now().date_naive()
    }
}
