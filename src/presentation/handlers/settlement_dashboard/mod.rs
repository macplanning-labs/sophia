pub mod legacy;
pub mod api;

pub use legacy::*;
pub use api::*;

use chrono::{NaiveDate, Datelike};
use serde::Deserialize;
use crate::domain::services::settlement_dashboard::{
    SettlementSummary, SettlementViewRow,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClientOption {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PartnerOption {
    pub partner_id: String,
    pub name: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectOption {
    pub project_id: String,
    pub name: String,
}

// ── フィルタフォーム ──

/// 空文字列を None として扱うデシリアライザ
pub fn deserialize_option_i64_from_empty<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(ref v) if v.is_empty() => Ok(None),
        Some(v) => v.parse::<i64>().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

/// 空文字列を None として扱うデシリアライザ（String版）
pub fn deserialize_option_string_from_empty<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        Some(ref v) if v.trim().is_empty() => Ok(None),
        other => Ok(other),
    }
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct DashboardFilter {
    #[serde(default, deserialize_with = "deserialize_option_string_from_empty")]
    pub month: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_i64_from_empty")]
    pub client_id: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_option_string_from_empty")]
    pub partner_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_string_from_empty")]
    pub project_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_string_from_empty")]
    pub status: Option<String>,
}

// ── 一括発行フォーム ──

#[derive(Debug, serde::Deserialize)]
pub struct IssueForm {
    pub selected_ids: String,  // カンマ区切りの partner_contract_id
    pub target_month: String,
}

// ── ヘルパー ──

/// 過去N ヶ月 + 当月の月リストを生成
pub fn generate_month_list(today: NaiveDate, months_back: u32) -> Vec<(String, String)> {
    let mut list = Vec::new();
    for i in 0..=months_back {
        let m = today.month() as i32 - i as i32;
        let (y, m) = if m <= 0 {
            (today.year() - 1, (m + 12) as u32)
        } else {
            (today.year(), m as u32)
        };
        let value = format!("{}-{:02}-01", y, m);
        let label = format!("{}年{:02}月", y, m);
        list.push((value, label));
    }
    list
}

// ── SPA用 JSON APIレスポンス ──

/// JSON APIレスポンス: 月次確定データ
#[derive(serde::Serialize)]
pub struct SettlementApiResponse {
    pub rows: Vec<SettlementViewRow>,
    pub summary: SettlementSummary,
    pub current_month: String,
}

/// JSON APIレスポンス: フィルタ用マスタデータ
#[derive(serde::Serialize)]
pub struct FiltersApiResponse {
    pub available_months: Vec<MonthOption>,
    pub clients: Vec<ClientOption>,
    pub partners: Vec<PartnerOption>,
    pub projects: Vec<ProjectOption>,
}

#[derive(serde::Serialize)]
pub struct MonthOption {
    pub value: String,
    pub label: String,
}

/// POST /api/settlement/issue-invoices — 請求書一括発行（JSON版）
#[derive(Debug, serde::Deserialize)]
pub struct ApiIssueForm {
    pub selected_ids: Vec<i64>,
    pub target_month: String,
}
