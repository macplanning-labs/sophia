/// domain/models/partner.rs — パートナー（発注先）エンティティ

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// パートナー（自社が注文を出す会社）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Partner {
    pub partner_id: String,
    pub name: String,
    pub name_kana: String,
    pub postal_code: String,
    pub address: String,
    pub tel: String,
    pub fax: String,
    pub email: String,
    pub report_email: String,
    pub cc: String,
    pub bcc: String,
    pub representative_name: String,
    pub representative_name_kana: String,
    pub representative_position: String,
    pub responsible_person: String,
    pub contact_person: String,
    pub registration_no: String,
    pub staff_contact_id: Option<i64>,
    pub attachment_file: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
    pub peppol_participant_id: String,
}

/// パートナー登録・編集フォーム
#[derive(Debug, Deserialize)]
pub struct PartnerForm {
    pub name: String,
    pub name_kana: Option<String>,
    pub postal_code: Option<String>,
    pub address: Option<String>,
    pub tel: Option<String>,
    pub fax: Option<String>,
    pub email: String,
    pub report_email: Option<String>,
    pub cc: Option<String>,
    pub bcc: Option<String>,
    pub representative_name: Option<String>,
    pub representative_name_kana: Option<String>,
    pub representative_position: Option<String>,
    pub responsible_person: Option<String>,
    pub contact_person: Option<String>,
    pub registration_no: Option<String>,
    pub bank_name: Option<String>,
    pub bank_branch: Option<String>,
    pub account_type: Option<String>,
    pub account_number: Option<String>,
    pub account_name: Option<String>,
}

/// 基本契約進捗ステータス
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ContractProgressStatus {
    Invited,
    InfoDone,
    ContractSent,
    PendingApproval,
    Completed,
}

impl ContractProgressStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Invited => "INVITED",
            Self::InfoDone => "INFO_DONE",
            Self::ContractSent => "CONTRACT_SENT",
            Self::PendingApproval => "PENDING_APPROVAL",
            Self::Completed => "COMPLETED",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Invited => "招待済み",
            Self::InfoDone => "基本情報登録済み",
            Self::ContractSent => "基本契約送信済み",
            Self::PendingApproval => "承諾待ち",
            Self::Completed => "締結完了",
        }
    }
}

impl std::str::FromStr for ContractProgressStatus {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "INVITED" => Self::Invited,
            "INFO_DONE" => Self::InfoDone,
            "CONTRACT_SENT" => Self::ContractSent,
            "PENDING_APPROVAL" => Self::PendingApproval,
            "COMPLETED" => Self::Completed,
            _ => Self::Invited,
        })
    }
}
