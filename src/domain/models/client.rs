/// domain/models/client.rs — クライアント（取引先）エンティティ

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// クライアント（取引先）— 旧 Customer + BillingCustomer の統合
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Client {
    pub id: i64,
    pub name: String,
    pub contact_person: String,
    pub email: String,
    pub cc_email: String,
    pub phone: String,
    pub postal_code: String,
    pub address: String,
    pub representative_name: String,
    pub representative_title: String,
    pub registration_no: String,
    pub url: String,
    pub report_email: String,
    pub work_report_email: String,
    pub invoice_email: String,
    pub edi_system_type: String,
    pub edi_notification_email: String,
    pub peppol_participant_id: String,
}

/// クライアント登録・編集フォーム
#[derive(Debug, Deserialize)]
pub struct ClientForm {
    pub name: String,
    pub contact_person: Option<String>,
    pub email: Option<String>,
    pub cc_email: Option<String>,
    pub phone: Option<String>,
    pub postal_code: Option<String>,
    pub address: Option<String>,
    pub representative_name: Option<String>,
    pub representative_title: Option<String>,
    pub registration_no: Option<String>,
    pub url: Option<String>,
    pub report_email: Option<String>,
    pub work_report_email: Option<String>,
    pub invoice_email: Option<String>,
    pub edi_system_type: Option<String>,
    pub edi_notification_email: Option<String>,
}
