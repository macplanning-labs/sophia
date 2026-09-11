/// domain/models/received_email.rs — 受信メールモデル
///
/// t_received_email テーブルのマッピング。
/// パートナーからの稼働報告メールをWebhookで受信し管理する。

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
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
    // mail_pipeline（migrations/019）で追加された列
    pub source_type: String,
    pub client_id: Option<i64>,
    pub retry_count: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub needs_manual_review: bool,
    pub review_notified_at: Option<DateTime<Utc>>,
    pub drive_file_id: String,
    pub drive_link: String,
    pub parsed_data: Option<serde_json::Value>,
}
