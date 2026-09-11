/// domain/value_objects.rs — 値オブジェクト
///
/// 精算計算ロジックの唯一の実装箇所。
/// 全ての精算金額計算は SettlementTerms::calculate_adjustment() を通して行う。

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// 精算条件（値オブジェクト）。
///
/// SES契約における精算計算ロジックの唯一の実装箇所。
/// ClientContract, PartnerContract, PurchaseOrderItem 等から生成して使用する。
///
/// フィールド:
///   - lower_limit_hours: 精算下限時間（例: 140.0）
///   - upper_limit_hours: 精算上限時間（例: 180.0）
///   - fixed_hours: 固定精算時間（「150時間固定」契約用、任意）
///   - deduction_rate: 控除単価（円/h）— 下限を割った際の1時間あたり減額
///   - overtime_rate: 超過単価（円/h）— 上限を超えた際の1時間あたり増額
#[derive(Debug, Clone)]
pub struct SettlementTerms {
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
}

impl SettlementTerms {
    /// 精算超過時間（契約上限または固定時間を超えた実績時間。範囲内なら 0）。
    pub fn excess_hours(&self, actual_hours: Decimal) -> Decimal {
        if let Some(fixed) = self.fixed_hours {
            let diff = actual_hours - fixed;
            if diff > Decimal::ZERO {
                diff.round_dp(2)
            } else {
                Decimal::ZERO
            }
        } else if actual_hours > self.upper_limit_hours {
            (actual_hours - self.upper_limit_hours).round_dp(2)
        } else {
            Decimal::ZERO
        }
    }

    /// 調整金を計算する（正=超過加算, 負=控除減額）。
    ///
    /// 固定精算時間（fixed_hours）が設定されている場合:
    ///     fixed_hours との差分で計算。
    ///
    /// 上下限割の場合:
    ///     下限未満 → (下限 - 実績) × deduction_rate を減額
    ///     上限超過 → (実績 - 上限) × overtime_rate を加算
    ///     範囲内 → 調整金なし
    pub fn calculate_adjustment(&self, actual_hours: Decimal) -> i32 {
        if let Some(fixed) = self.fixed_hours {
            let diff = actual_hours - fixed;
            if diff < Decimal::ZERO {
                return -(diff.abs() * Decimal::from(self.deduction_rate))
                    .to_i32()
                    .unwrap_or(0);
            } else if diff > Decimal::ZERO {
                return (diff * Decimal::from(self.overtime_rate))
                    .to_i32()
                    .unwrap_or(0);
            }
            return 0;
        }

        // 上下限割
        if actual_hours < self.lower_limit_hours {
            let shortage = self.lower_limit_hours - actual_hours;
            -(shortage * Decimal::from(self.deduction_rate))
                .to_i32()
                .unwrap_or(0)
        } else if actual_hours > self.upper_limit_hours {
            let excess = actual_hours - self.upper_limit_hours;
            (excess * Decimal::from(self.overtime_rate))
                .to_i32()
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// 金額 = (工数 × 基本単価) + 調整金。
    pub fn calculate_amount(
        &self,
        base_rate: i32,
        effort: Decimal,
        actual_hours: Decimal,
    ) -> i32 {
        (effort * Decimal::from(base_rate))
            .to_i32()
            .unwrap_or(0)
            + self.calculate_adjustment(actual_hours)
    }
}

/// 精算条件フィールドを持つモデルが実装するトレイト。
/// PartnerContract, ClientContract, ReceivedOrderItem 等で使用。
pub trait SettlementFields {
    fn lower_limit_hours(&self) -> Decimal;
    fn upper_limit_hours(&self) -> Decimal;
    fn fixed_hours(&self) -> Option<Decimal>;
    fn deduction_rate(&self) -> i32;
    fn overtime_rate(&self) -> i32;

    /// SettlementTerms 値オブジェクトに変換する。
    fn to_settlement_terms(&self) -> SettlementTerms {
        SettlementTerms {
            lower_limit_hours: self.lower_limit_hours(),
            upper_limit_hours: self.upper_limit_hours(),
            fixed_hours: self.fixed_hours(),
            deduction_rate: self.deduction_rate(),
            overtime_rate: self.overtime_rate(),
        }
    }
}

/// 適格請求書発行事業者登録番号の形式チェック（"T" + 13桁数字）。
/// 空文字は未登録として許容する（既存データ・任意入力のため）。
pub fn is_valid_qualified_invoice_registration_no(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"^T\d{13}$").expect("registration_no regex"));
    RE.is_match(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registration_no_valid() {
        assert!(is_valid_qualified_invoice_registration_no(""));
        assert!(is_valid_qualified_invoice_registration_no("T1234567890123"));
    }

    #[test]
    fn test_registration_no_invalid() {
        assert!(!is_valid_qualified_invoice_registration_no("1234567890123"));
        assert!(!is_valid_qualified_invoice_registration_no("T123456789012"));
        assert!(!is_valid_qualified_invoice_registration_no("T12345678901234"));
        assert!(!is_valid_qualified_invoice_registration_no("Xabcdefghijklm"));
    }

    #[test]
    fn test_range_within_limits() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        assert_eq!(terms.calculate_adjustment(Decimal::from(160)), 0);
    }

    #[test]
    fn test_range_below_lower() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        // 130h → 下限 140 - 130 = 10h × 3000 = -30000
        assert_eq!(terms.calculate_adjustment(Decimal::from(130)), -30000);
    }

    #[test]
    fn test_range_above_upper() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        // 190h → 190 - 上限 180 = 10h × 3000 = +30000
        assert_eq!(terms.calculate_adjustment(Decimal::from(190)), 30000);
    }

    #[test]
    fn test_fixed_settlement() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(0),
            upper_limit_hours: Decimal::from(0),
            fixed_hours: Some(Decimal::from(150)),
            deduction_rate: 3500,
            overtime_rate: 3500,
        };
        // 155h → 155 - 150 = 5h × 3500 = +17500
        assert_eq!(terms.calculate_adjustment(Decimal::from(155)), 17500);
        // 145h → 145 - 150 = -5h × 3500 = -17500
        assert_eq!(terms.calculate_adjustment(Decimal::from(145)), -17500);
        // 150h → 差分0 → 調整金0
        assert_eq!(terms.calculate_adjustment(Decimal::from(150)), 0);
    }

    #[test]
    fn test_calculate_amount() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        // 工数1.00 × 単価550000 + 調整金0 = 550000
        let amount = terms.calculate_amount(
            550000,
            Decimal::from(1),
            Decimal::from(160),
        );
        assert_eq!(amount, 550000);

        // 工数1.00 × 単価550000 + 超過(190-180)*3000 = 550000 + 30000
        let amount = terms.calculate_amount(
            550000,
            Decimal::from(1),
            Decimal::from(190),
        );
        assert_eq!(amount, 580000);
    }

    #[test]
    fn test_excess_hours_within_range_is_zero() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        assert_eq!(terms.excess_hours(Decimal::from(177)), Decimal::ZERO);
    }

    #[test]
    fn test_excess_hours_above_upper() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        assert_eq!(terms.excess_hours(Decimal::from(190)), Decimal::from(10));
    }

    #[test]
    fn test_boundary_exact_limits() {
        let terms = SettlementTerms {
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 3000,
            overtime_rate: 3000,
        };
        // 境界値: ちょうど下限 → 調整金0
        assert_eq!(terms.calculate_adjustment(Decimal::from(140)), 0);
        // 境界値: ちょうど上限 → 調整金0
        assert_eq!(terms.calculate_adjustment(Decimal::from(180)), 0);
    }
}
