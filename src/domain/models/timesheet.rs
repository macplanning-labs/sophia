/// domain/models/timesheet.rs — 月次稼働報告・受信メール

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 稼働報告ステータス
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimesheetStatus {
    Pending,
    Uploaded,
    Parsed,
    Approved,
    Sent,
    Alert,
    Error,
}

impl TimesheetStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Uploaded => "UPLOADED",
            Self::Parsed => "PARSED",
            Self::Approved => "APPROVED",
            Self::Sent => "SENT",
            Self::Alert => "ALERT",
            Self::Error => "ERROR",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Pending => "未提出",
            Self::Uploaded => "受領済",
            Self::Parsed => "解析済",
            Self::Approved => "承認済",
            Self::Sent => "送付済",
            Self::Alert => "要確認",
            Self::Error => "エラー",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "PENDING" => Self::Pending,
            "UPLOADED" => Self::Uploaded,
            "PARSED" => Self::Parsed,
            "APPROVED" => Self::Approved,
            "SENT" => Self::Sent,
            "ALERT" => Self::Alert,
            "ERROR" => Self::Error,
            _ => Self::Pending,
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Pending => "bg-secondary",
            Self::Uploaded => "bg-primary",
            Self::Parsed => "bg-info",
            Self::Approved => "bg-success",
            Self::Sent => "bg-dark",
            Self::Alert => "bg-warning text-dark",
            Self::Error => "bg-danger",
        }
    }
}

/// 基準時間チェック結果
#[derive(Debug, Clone, PartialEq)]
pub enum HoursCheckResult {
    Ok,
    Below,
    Above,
}

impl HoursCheckResult {
    pub fn display(&self) -> &'static str {
        match self {
            Self::Ok => "範囲内",
            Self::Below => "下限未満",
            Self::Above => "上限超過",
        }
    }
}

/// 月次稼働報告
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MonthlyTimesheet {
    pub id: i64,
    /// 案件（受注契約）紐付けの場合のみSome。社員自己申告の場合はNone（employee_idを使う）
    pub client_contract_id: Option<i64>,
    /// 案件に紐づかない社員自己申告の場合のみSome。client_contract_idとは排他（chk_timesheet_owner）
    pub employee_id: Option<i64>,
    /// Googleスプレッドシート連携で複製したテンプレートのファイルID（未使用時は空文字）
    pub sheet_file_id: String,
    pub target_month: NaiveDate,
    pub status: String,
    pub total_hours: Decimal,
    pub work_days: i32,
    pub overtime_hours: Decimal,
    pub night_hours: Decimal,
    pub holiday_hours: Decimal,
    pub excel_file: String,
    pub pdf_file: String,
    pub original_filename: String,
    pub drive_file_id: String,
    pub uploaded_by_id: Option<i64>,
    pub uploaded_at: Option<DateTime<Utc>>,
    /// 日ごとの内訳（日付・開始・終了・休憩・休みフラグ）。社員自己申告フォームで使用
    pub daily_data: Option<serde_json::Value>,
    pub error_message: String,
    pub sent_to_client_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 受信メール
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ReceivedEmail {
    pub id: i64,
    pub message_id: String,
    pub from_email: String,
    pub from_name: String,
    pub subject: String,
    pub received_at: DateTime<Utc>,
    pub body_text: String,
    pub partner_id: Option<String>,
    pub status: String,
    pub timesheet_id: Option<i64>,
    pub error_message: String,
    pub attachment_filename: String,
    pub attachment_file: String,
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

/// 稼働報告登録フォーム
#[derive(Debug, Deserialize)]
pub struct TimesheetForm {
    pub client_contract_id: i64,
    pub target_month: NaiveDate,
    pub total_hours: Decimal,
    pub work_days: i32,
    pub overtime_hours: Option<Decimal>,
    pub night_hours: Option<Decimal>,
    pub holiday_hours: Option<Decimal>,
}
