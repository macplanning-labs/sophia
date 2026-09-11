/// domain/services/tax_calculation.rs — 税率別の消費税額計算
///
/// JP PINTのルール（1インボイスにつき税率ごとに1回のみ端数処理）を満たすため、
/// 明細を税率でグルーピングしてから税率ごとに1回だけ端数処理する。
/// 端数処理は既存ロジック（notices.rs等）を踏襲しfloor（切り捨て）。

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct TaxBreakdown {
    pub rate: Decimal,
    pub taxable_amount: i32,
    pub tax_amount: i32,
}

/// (税率, 税抜金額) のリストを税率でグルーピングし、税率ごとに1回だけ端数処理して返す。
pub fn calculate_tax_breakdown(items: &[(Decimal, i32)]) -> Vec<TaxBreakdown> {
    let mut groups: BTreeMap<Decimal, i32> = BTreeMap::new();
    for (rate, amount) in items {
        *groups.entry(*rate).or_insert(0) += amount;
    }

    groups
        .into_iter()
        .map(|(rate, taxable_amount)| {
            let tax_amount = (Decimal::from(taxable_amount) * rate / Decimal::from(100))
                .floor()
                .to_i32()
                .unwrap_or(0);
            TaxBreakdown { rate, taxable_amount, tax_amount }
        })
        .collect()
}

/// 税率別内訳の消費税額合計（既存のsubtotal/tax_amount/total互換用）
pub fn total_tax_amount(breakdown: &[TaxBreakdown]) -> i32 {
    breakdown.iter().map(|b| b.tax_amount).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_rate_matches_legacy_floor_division() {
        let items = vec![(Decimal::from(10), 12345)];
        let breakdown = calculate_tax_breakdown(&items);
        assert_eq!(breakdown.len(), 1);
        assert_eq!(breakdown[0].rate, Decimal::from(10));
        assert_eq!(breakdown[0].taxable_amount, 12345);
        // 旧ロジック: (12345 as f64 * 0.1).floor() == 1234
        assert_eq!(breakdown[0].tax_amount, 1234);
        assert_eq!(total_tax_amount(&breakdown), 1234);
    }

    #[test]
    fn mixed_rates_are_rounded_independently() {
        let items = vec![
            (Decimal::from(10), 10000),
            (Decimal::from(8), 10000),
            (Decimal::from(10), 5000),
        ];
        let breakdown = calculate_tax_breakdown(&items);
        assert_eq!(breakdown.len(), 2);

        let ten_pct = breakdown.iter().find(|b| b.rate == Decimal::from(10)).unwrap();
        assert_eq!(ten_pct.taxable_amount, 15000);
        assert_eq!(ten_pct.tax_amount, 1500);

        let eight_pct = breakdown.iter().find(|b| b.rate == Decimal::from(8)).unwrap();
        assert_eq!(eight_pct.taxable_amount, 10000);
        assert_eq!(eight_pct.tax_amount, 800);

        assert_eq!(total_tax_amount(&breakdown), 2300);
    }

    #[test]
    fn empty_items_yield_empty_breakdown() {
        assert!(calculate_tax_breakdown(&[]).is_empty());
    }
}
