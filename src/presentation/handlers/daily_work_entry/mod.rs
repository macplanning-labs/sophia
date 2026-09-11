pub mod engineer;
pub mod work;

pub use engineer::*;
pub use work::*;

use chrono::{NaiveDate, NaiveTime};
use serde::Deserialize;
use sqlx::PgPool;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::middleware::role::AuthUser;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 型定義
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Deserialize)]
pub struct WorkEntryQuery {
    pub engineer_id: i64,
    pub year: i32,
    pub month: i32,
}

#[derive(Debug, Deserialize)]
pub struct SaveWorkEntriesRequest {
    pub engineer_id: i64,
    pub entries: Vec<WorkEntryInput>,
}

#[derive(Debug, Deserialize)]
pub struct WorkEntryInput {
    pub work_date: NaiveDate,
    pub start_time: Option<String>,  // "HH:MM" or null
    pub end_time: Option<String>,    // "HH:MM" or null
    pub break_minutes: Option<i32>,
    pub work_description: Option<String>,
    pub is_holiday: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct InvitationRequest {
    pub engineer_id: i64,
    pub email: String,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 計算ロジック・ヘルパー
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 日ごとの実働時間（分）を計算
pub fn calc_actual_minutes(start: Option<NaiveTime>, end: Option<NaiveTime>, break_min: i32, is_holiday: bool) -> i32 {
    if is_holiday {
        return 0;
    }
    match (start, end) {
        (Some(s), Some(e)) => {
            let work_min = (e - s).num_minutes() as i32;
            (work_min - break_min).max(0)
        }
        _ => 0,
    }
}

/// "HH:MM" 文字列を NaiveTime にパース
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(s, "%H:%M").ok()
}

/// エンジニアへのアクセス権限があるかチェック
pub async fn check_permission(pool: &PgPool, auth_user: &AuthUser, engineer_id: i64) -> bool {
    // Adminは常に許可
    if auth_user.role() == crate::presentation::middleware::role::Role::Admin {
        return true;
    }

    // エンジニア本人の場合
    if let Some(own_id) = auth_user.engineer_id() {
        return own_id == engineer_id;
    }

    // パートナー担当者の場合
    if let Some(partner_id) = auth_user.partner_id() {
        return order_repo::engineer_belongs_to_partner(pool, engineer_id, partner_id).await.unwrap_or(false);
    }

    false
}
