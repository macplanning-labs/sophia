/// domain/services/settlement.rs — 精算計算サービス
///
/// SettlementTerms 値オブジェクトのファサード。
/// 受発注明細の精算金額計算を統一的に行う。

use rust_decimal::Decimal;
use crate::domain::value_objects::SettlementFields;

/// 精算金額を計算する（明細の actual_hours から）
pub fn calculate_item_price<T: SettlementFields>(
    item: &T,
    base_rate: i32,
    effort: Decimal,
    actual_hours: Decimal,
) -> i32 {
    let terms = item.to_settlement_terms();
    terms.calculate_amount(base_rate, effort, actual_hours)
}

/// 調整金のみを計算する
pub fn calculate_adjustment<T: SettlementFields>(
    item: &T,
    actual_hours: Decimal,
) -> i32 {
    let terms = item.to_settlement_terms();
    terms.calculate_adjustment(actual_hours)
}
