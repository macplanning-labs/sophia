/// domain/models/project.rs — 案件（プロジェクト）エンティティ

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::domain::services::business_day::{
    ReportDeadlineHolidayRule, ReportDeadlineSettings, ReportDeadlineType,
};

/// 案件（プロジェクト）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Project {
    pub project_id: String,
    pub client_id: i64,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    /// 稼働報告提出期限の種類: "RELATIVE" | "FIXED_DAY"
    pub report_deadline_type: String,
    /// RELATIVE: 月末からのN営業日前 / FIXED_DAY: 当月N日。未設定時はNULL（契約側にフォールバック）
    pub report_deadline_value: Option<i32>,
    /// FIXED_DAYが非営業日の場合の調整ルール: "PREVIOUS_BUSINESS_DAY" | "NEXT_BUSINESS_DAY"
    pub report_deadline_holiday_rule: Option<String>,
}

impl Project {
    /// `report_deadline_value` が設定されている場合のみ `ReportDeadlineSettings` を組み立てる。
    /// `None` はプロジェクト単位の期限が未設定であることを示し、呼び出し側は契約側
    /// （`m_client_contract.report_deadline_days_before`）にフォールバックする。
    pub fn report_deadline_settings(&self) -> Option<ReportDeadlineSettings> {
        let value = self.report_deadline_value?;
        Some(ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::from_str(&self.report_deadline_type),
            value,
            holiday_rule: self.report_deadline_holiday_rule.as_deref()
                .and_then(ReportDeadlineHolidayRule::from_str),
        })
    }
}

/// 案件 + クライアント名（一覧表示用）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ProjectWithClient {
    pub project_id: String,
    pub client_id: i64,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub client_name: String,
    pub report_deadline_type: String,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
}

/// 案件登録・編集フォーム
#[derive(Debug, Deserialize)]
pub struct ProjectForm {
    pub project_id: String,
    pub client_id: i64,
    pub name: String,
    pub description: Option<String>,
}
