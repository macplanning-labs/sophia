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

impl Client {
    /// EDIシステムを保有しているか
    ///
    /// `edi_system_type`は"なし"/"EDI-OASIS"/"メール"の3択（`masters.rs::EDI_OPTIONS`）だが、
    /// "メール"は単に注文書がメールで連携される旨のメモであり、Phase2が年月ポーリング可能な
    /// 実在のEDIシステムではない。非空判定だと"メール"もEDI保有扱いになってしまうため、
    /// 実際にポーリング対象となる"EDI_OASIS"との完全一致で判定する。
    pub fn has_edi(&self) -> bool {
        self.edi_system_type == "EDI_OASIS"
    }

    /// 請求書の作成・送付が必要か（EDI非保有の場合のみ）
    pub fn needs_invoice(&self) -> bool {
        !self.has_edi()
    }
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
