/// domain/models/engineer.rs — エンジニア（要員）エンティティ

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 所属区分
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AffiliationType {
    Internal,
    Partner,
}

impl AffiliationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Internal => "INTERNAL",
            Self::Partner => "PARTNER",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Internal => "自社社員",
            Self::Partner => "パートナー",
        }
    }
}

impl std::str::FromStr for AffiliationType {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "INTERNAL" => Self::Internal,
            _ => Self::Partner,
        })
    }
}

/// エンジニア（要員）— 自社社員・パートナー社員の統合管理
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Engineer {
    pub id: i64,
    pub name: String,
    pub name_kana: String,
    pub affiliation_type: String,
    pub partner_id: Option<String>,
    pub employee_id: String,
    pub email: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Engineer {
    /// 所属区分の表示名
    pub fn affiliation_display(&self) -> &str {
        self.affiliation_type.parse::<AffiliationType>()
            .unwrap_or(AffiliationType::Partner)
            .display()
    }

    /// 自社社員か
    pub fn is_internal(&self) -> bool {
        self.affiliation_type == "INTERNAL"
    }
}

/// エンジニア登録・編集フォーム
#[derive(Debug, Deserialize)]
pub struct EngineerForm {
    pub name: String,
    pub name_kana: Option<String>,
    pub affiliation_type: String,
    pub partner_id: Option<String>,
    pub employee_id: Option<String>,
    pub email: Option<String>,
}
