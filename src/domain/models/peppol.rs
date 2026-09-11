/// domain/models/peppol.rs — Peppol送受信ログ（t_peppol_transmission に対応）

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PeppolTransmission {
    pub id: i64,
    pub direction: String,
    pub document_type: String,
    pub related_table: String,
    pub related_id: String,
    pub peppol_message_id: String,
    pub participant_id: String,
    pub status: String,
    pub request_payload: Option<serde_json::Value>,
    pub response_payload: Option<serde_json::Value>,
    pub error_message: String,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
