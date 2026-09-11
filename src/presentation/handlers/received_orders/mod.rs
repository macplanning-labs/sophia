pub mod legacy;
pub mod action;
pub mod api;

pub use legacy::*;
pub use action::*;
pub use api::*;

use rust_decimal::Decimal;
use crate::infrastructure::repositories::order_repo::ReceivedOrderRow;

// ── テンプレート用 ──

impl ReceivedOrderRow {
    /// 対象月を「YYYY-MM」形式で表示
    pub fn target_month_display(&self) -> String {
        self.target_month.format("%Y-%m").to_string()
    }

    /// 合計金額を「¥1,234,567」形式で表示
    pub fn total_amount_display(&self) -> String {
        match self.total_amount {
            Some(amt) if amt != 0 => format!("¥{}", format_number(amt)),
            Some(_) => "—".to_string(),
            None => "—".to_string(),
        }
    }
}

// ── フィルタパラメータ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct OrderFilter {
    pub client: Option<String>,
    pub status: Option<String>,
    pub month: Option<String>,
}

// ── ユーティリティ ──

/// 数値を3桁区切りでフォーマットする（例: 1234567 → "1,234,567"）
pub fn format_number(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if n < 0 {
        result.push('-');
    }
    result.chars().rev().collect()
}
