//! アサイン(受注契約+発注契約)の利益計算と入力検証(UI刷新 2-6・2-7 / DEMO-000139・000140)。
//!
//! 金額は、請求書・支払通知と同じ既存の精算計算 `SettlementTerms::calculate_amount` を使う(再実装しない)。
//! 純関数だけで、DBは使わない。目安(直受け20%・下請け8%)は基準ではなく参考で、赤字でもアサインは作れる。

use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::domain::value_objects::SettlementTerms;

fn one() -> Decimal {
    Decimal::ONE
}

/// 単価と精算幅(受注側・発注側で同じ形)
#[derive(Debug, Clone, Deserialize)]
pub struct TermsInput {
    /// 基本単価(月額・円)
    pub base_rate: i32,
    /// 工数(1.0 = 1人月)
    #[serde(default = "one")]
    pub effort: Decimal,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    #[serde(default)]
    pub fixed_hours: Option<Decimal>,
    #[serde(default)]
    pub deduction_rate: i32,
    #[serde(default)]
    pub overtime_rate: i32,
}

impl TermsInput {
    fn settlement(&self) -> SettlementTerms {
        SettlementTerms {
            lower_limit_hours: self.lower_limit_hours,
            upper_limit_hours: self.upper_limit_hours,
            fixed_hours: self.fixed_hours,
            deduction_rate: self.deduction_rate,
            overtime_rate: self.overtime_rate,
        }
    }

    /// 稼働時間が `hours` のときの月額(工数×単価 + 精算の調整金)
    pub fn amount_at(&self, hours: Decimal) -> i32 {
        self.settlement().calculate_amount(self.base_rate, self.effort, hours)
    }

    /// 標準の稼働時間。固定時間があればそれ、なければ精算幅の中間
    pub fn standard_hours(&self) -> Decimal {
        self.fixed_hours
            .unwrap_or_else(|| (self.lower_limit_hours + self.upper_limit_hours) / Decimal::from(2))
    }

    /// 精算前の月額(工数×単価)。前回単価との比較や案件全体の集計に使う
    pub fn monthly_base(&self) -> i32 {
        (self.effort * Decimal::from(self.base_rate)).round().to_i32().unwrap_or(0)
    }
}

/// 単価・精算幅の入力検証
pub fn validate_terms(t: &TermsInput, label: &str) -> Result<(), String> {
    if t.base_rate <= 0 {
        return Err(format!("{label}の単価は1円以上で入力してください"));
    }
    if t.effort <= Decimal::ZERO {
        return Err(format!("{label}の工数は0より大きい値で入力してください"));
    }
    if t.lower_limit_hours < Decimal::ZERO || t.upper_limit_hours < t.lower_limit_hours {
        return Err(format!("{label}の精算幅は、下限が0以上で、上限が下限以上になるよう入力してください"));
    }
    if t.deduction_rate < 0 || t.overtime_rate < 0 {
        return Err(format!("{label}の控除・超過の単価は0以上で入力してください"));
    }
    Ok(())
}

/// 期間の検証(終了日は開始日以後)
pub fn validate_period(start: NaiveDate, end: NaiveDate, label: &str) -> Result<(), String> {
    if end < start {
        return Err(format!("{label}の終了日は、開始日以後の日付にしてください"));
    }
    Ok(())
}

/// 粗利率(%)。売上が0以下なら None
pub fn margin_pct(revenue: i32, cost: i32) -> Option<f64> {
    if revenue <= 0 {
        return None;
    }
    Some(((revenue - cost) as f64 / revenue as f64) * 100.0)
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Monthly {
    pub revenue: i32,
    /// 発注(パートナー要員)があるときだけ。自社社員は原価(給与)が分からないので None
    pub cost: Option<i32>,
    pub gross_profit: Option<i32>,
    pub margin_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Scenario {
    pub label: String,
    pub hours: Decimal,
    pub revenue: i32,
    pub cost: Option<i32>,
    pub margin_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Target {
    /// 案件の商流("DIRECT" / "SUBCONTRACT")。未設定なら None
    pub flow: Option<String>,
    pub target_pct: Option<i32>,
    /// 粗利率 − 目安(ポイント)
    pub diff_pct: Option<f64>,
    /// OK | BELOW | UNSET(商流が未設定) | NOT_APPLICABLE(原価が分からない=自社社員)
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Warning {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectSide {
    pub revenue: i64,
    pub cost: i64,
    pub gross_profit: i64,
    pub margin_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectTotal {
    pub before: ProjectSide,
    pub after: ProjectSide,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProfitPreview {
    pub monthly: Monthly,
    pub target: Target,
    pub hours_scenarios: Vec<Scenario>,
    pub warnings: Vec<Warning>,
    pub project_total: ProjectTotal,
}

fn yen(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    let body: String = out.chars().rev().collect();
    if n < 0 { format!("-{body}") } else { body }
}

fn project_side(revenue: i64, cost: i64) -> ProjectSide {
    ProjectSide {
        revenue,
        cost,
        gross_profit: revenue - cost,
        margin_pct: if revenue > 0 { Some(round1((revenue - cost) as f64 / revenue as f64 * 100.0)) } else { None },
    }
}

/// 案件全体の粗利と、商流ごとの目安との比較(アサイン編成画面の案件カード用)。
/// `NOT_APPLICABLE`: 売上が無い(まだアサインが無い)/ `UNSET`: 商流が未設定 / `OK` / `BELOW`
pub fn project_target_status(
    flow: Option<&str>,
    revenue: i64,
    cost: i64,
    targets: (i32, i32),
) -> (Option<f64>, Option<i32>, Option<f64>, &'static str) {
    if revenue <= 0 {
        return (None, None, None, "NOT_APPLICABLE");
    }
    let margin = round1((revenue - cost) as f64 / revenue as f64 * 100.0);
    let target = match flow {
        Some("DIRECT") => Some(targets.0),
        Some("SUBCONTRACT") => Some(targets.1),
        _ => None,
    };
    match target {
        None => (Some(margin), None, None, "UNSET"),
        Some(t) => (
            Some(margin),
            Some(t),
            Some(round1(margin - t as f64)),
            if margin >= t as f64 { "OK" } else { "BELOW" },
        ),
    }
}

/// 利益の試算
/// - `flow`: 案件の商流。`targets` = (直受け%, 下請け%)
/// - `prev_price`: 同じ要員の前回の月額(なければ None)
/// - `project_before`: この要員を加える前の、案件の(月額売上, 月額原価)
pub fn evaluate(
    client: &TermsInput,
    partner: Option<&TermsInput>,
    flow: Option<&str>,
    targets: (i32, i32),
    prev_price: Option<i32>,
    project_before: (i64, i64),
) -> ProfitPreview {
    let std_hours = client.standard_hours();
    let revenue = client.amount_at(std_hours);
    let cost = partner.map(|p| p.amount_at(std_hours));
    let gross = cost.map(|c| revenue - c);
    let margin = cost.and_then(|c| margin_pct(revenue, c)).map(round1);
    let monthly = Monthly { revenue, cost, gross_profit: gross, margin_pct: margin };

    // 目安との比較
    let target_pct = match flow {
        Some("DIRECT") => Some(targets.0),
        Some("SUBCONTRACT") => Some(targets.1),
        _ => None,
    };
    let (diff_pct, status) = match (margin, target_pct, cost) {
        (_, _, None) => (None, "NOT_APPLICABLE"),
        (_, None, _) => (None, "UNSET"),
        (Some(m), Some(t), _) => (Some(round1(m - t as f64)), if m >= t as f64 { "OK" } else { "BELOW" }),
        (None, Some(_), _) => (None, "UNSET"),
    };
    let target = Target { flow: flow.map(|f| f.to_string()), target_pct, diff_pct, status };

    // 稼働時間別(下限割れ/標準/上限超過)。受注・発注とも同じ稼働時間で計算する
    let low = (client.lower_limit_hours - Decimal::from(10)).max(Decimal::ZERO);
    let high = client.upper_limit_hours + Decimal::from(10);
    let hours_scenarios: Vec<Scenario> = [("下限割れ(下限-10h)", low), ("標準", std_hours), ("上限超過(上限+10h)", high)]
        .into_iter()
        .map(|(label, h)| {
            let r = client.amount_at(h);
            let c = partner.map(|p| p.amount_at(h));
            Scenario {
                label: label.to_string(),
                hours: h,
                revenue: r,
                cost: c,
                margin_pct: c.and_then(|c| margin_pct(r, c)).map(round1),
            }
        })
        .collect();

    // 警告(情報であり、作成は妨げない)
    let mut warnings = Vec::new();
    if let Some(g) = gross {
        if g < 0 {
            warnings.push(Warning { code: "LOSS", message: format!("赤字です(月額 {}円)", yen(g as i64)) });
        }
    }
    if status == "BELOW" {
        if let (Some(m), Some(t)) = (margin, target_pct) {
            warnings.push(Warning {
                code: "BELOW_TARGET",
                message: format!("粗利率 {m:.1}% は目安の {t}% を下回っています"),
            });
        }
    }
    let new_base = client.monthly_base();
    if let Some(prev) = prev_price {
        if prev > 0 && new_base > 0 {
            if new_base as i64 >= prev as i64 * 10 {
                warnings.push(Warning {
                    code: "ORDER_OF_MAGNITUDE",
                    message: format!("単価が前回({}円)の10倍以上です。桁を確認してください", yen(prev as i64)),
                });
            } else if (new_base as i64) * 10 <= prev as i64 {
                warnings.push(Warning {
                    code: "ORDER_OF_MAGNITUDE",
                    message: format!("単価が前回({}円)の10分の1以下です。桁を確認してください", yen(prev as i64)),
                });
            } else if new_base != prev {
                let diff = new_base as i64 - prev as i64;
                let sign = if diff > 0 { "+" } else { "" };
                warnings.push(Warning {
                    code: "PREV_PRICE_DIFF",
                    message: format!("前回単価 {}円より {sign}{}円", yen(prev as i64), yen(diff)),
                });
            }
        }
    }
    if let Some(p) = partner {
        if p.lower_limit_hours != client.lower_limit_hours || p.upper_limit_hours != client.upper_limit_hours {
            warnings.push(Warning {
                code: "RANGE_MISMATCH",
                message: format!(
                    "受注と発注で精算幅が異なります({}-{}h / {}-{}h)",
                    client.lower_limit_hours.normalize(),
                    client.upper_limit_hours.normalize(),
                    p.lower_limit_hours.normalize(),
                    p.upper_limit_hours.normalize()
                ),
            });
        }
    }

    // 案件全体(この要員を加えた場合)
    let (before_rev, before_cost) = project_before;
    let project_total = ProjectTotal {
        before: project_side(before_rev, before_cost),
        after: project_side(before_rev + revenue as i64, before_cost + cost.unwrap_or(0) as i64),
    };

    ProfitPreview { monthly, target, hours_scenarios, warnings, project_total }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(base: i32, lower: i64, upper: i64) -> TermsInput {
        TermsInput {
            base_rate: base,
            effort: Decimal::ONE,
            lower_limit_hours: Decimal::from(lower),
            upper_limit_hours: Decimal::from(upper),
            fixed_hours: None,
            deduction_rate: 4000,
            overtime_rate: 4000,
        }
    }

    fn codes(p: &ProfitPreview) -> Vec<&'static str> {
        p.warnings.iter().map(|w| w.code).collect()
    }

    #[test]
    fn standard_hours_is_midpoint_or_fixed() {
        assert_eq!(terms(700_000, 140, 180).standard_hours(), Decimal::from(160));
        let mut t = terms(700_000, 140, 180);
        t.fixed_hours = Some(Decimal::from(150));
        assert_eq!(t.standard_hours(), Decimal::from(150));
    }

    #[test]
    fn profit_and_margin_with_partner() {
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(640_000, 140, 180)), Some("SUBCONTRACT"), (20, 8), None, (0, 0));
        assert_eq!(p.monthly.revenue, 700_000);
        assert_eq!(p.monthly.cost, Some(640_000));
        assert_eq!(p.monthly.gross_profit, Some(60_000));
        assert_eq!(p.monthly.margin_pct, Some(8.6));
        assert_eq!(p.target.status, "OK");
        assert_eq!(p.target.target_pct, Some(8));
        assert_eq!(p.target.diff_pct, Some(0.6));
        assert!(codes(&p).is_empty(), "{:?}", p.warnings);
    }

    #[test]
    fn below_target_is_a_warning_not_an_error() {
        // 粗利率 5.7% < 下請け8%
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(660_000, 140, 180)), Some("SUBCONTRACT"), (20, 8), None, (0, 0));
        assert_eq!(p.target.status, "BELOW");
        assert!(codes(&p).contains(&"BELOW_TARGET"));
        assert!(!codes(&p).contains(&"LOSS"));
    }

    #[test]
    fn loss_is_flagged_but_still_computed() {
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(720_000, 140, 180)), Some("DIRECT"), (20, 8), None, (0, 0));
        assert_eq!(p.monthly.gross_profit, Some(-20_000));
        assert!(codes(&p).contains(&"LOSS"));
        assert!(p.warnings.iter().any(|w| w.message.contains("-20,000円")));
    }

    #[test]
    fn direct_flow_uses_the_direct_target() {
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(600_000, 140, 180)), Some("DIRECT"), (20, 8), None, (0, 0));
        // 粗利率 14.3% < 直受け20%
        assert_eq!(p.target.target_pct, Some(20));
        assert_eq!(p.target.status, "BELOW");
    }

    #[test]
    fn unset_flow_and_employee_cases() {
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(640_000, 140, 180)), None, (20, 8), None, (0, 0));
        assert_eq!(p.target.status, "UNSET");
        let e = evaluate(&terms(700_000, 140, 180), None, Some("DIRECT"), (20, 8), None, (0, 0));
        assert_eq!(e.target.status, "NOT_APPLICABLE", "自社社員は原価が分からない");
        assert_eq!(e.monthly.cost, None);
        assert_eq!(e.monthly.margin_pct, None);
    }

    #[test]
    fn price_comparison_warnings() {
        let c = terms(700_000, 140, 180);
        let p10 = evaluate(&c, None, None, (20, 8), Some(70_000), (0, 0));
        assert!(codes(&p10).contains(&"ORDER_OF_MAGNITUDE"), "前回の10倍");
        let p01 = evaluate(&c, None, None, (20, 8), Some(7_000_000), (0, 0));
        assert!(codes(&p01).contains(&"ORDER_OF_MAGNITUDE"), "前回の1/10");
        let pd = evaluate(&c, None, None, (20, 8), Some(650_000), (0, 0));
        assert_eq!(codes(&pd), vec!["PREV_PRICE_DIFF"]);
        assert!(pd.warnings[0].message.contains("+50,000円"));
        let same = evaluate(&c, None, None, (20, 8), Some(700_000), (0, 0));
        assert!(codes(&same).is_empty(), "同額なら警告なし");
    }

    #[test]
    fn range_mismatch_is_flagged() {
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(640_000, 150, 190)), None, (20, 8), None, (0, 0));
        let w = p.warnings.iter().find(|w| w.code == "RANGE_MISMATCH").expect("精算幅のずれ");
        assert!(w.message.contains("140-180h / 150-190h"), "{}", w.message);
    }

    #[test]
    fn hours_scenarios_apply_the_settlement_adjustment() {
        let p = evaluate(&terms(700_000, 140, 180), None, None, (20, 8), None, (0, 0));
        assert_eq!(p.hours_scenarios.len(), 3);
        let low = &p.hours_scenarios[0];
        let std = &p.hours_scenarios[1];
        let high = &p.hours_scenarios[2];
        assert_eq!(low.hours, Decimal::from(130));
        assert_eq!(std.revenue, 700_000, "幅の中なら調整なし");
        assert!(low.revenue < 700_000, "下限割れは控除される");
        assert!(high.revenue > 700_000, "上限超過は加算される");
    }

    #[test]
    fn project_total_adds_this_assignment() {
        let p = evaluate(&terms(700_000, 140, 180), Some(&terms(640_000, 140, 180)), None, (20, 8), None, (1_000_000, 900_000));
        assert_eq!(p.project_total.before.gross_profit, 100_000);
        assert_eq!(p.project_total.after.revenue, 1_700_000);
        assert_eq!(p.project_total.after.cost, 1_540_000);
        assert_eq!(p.project_total.after.gross_profit, 160_000);
        assert_eq!(p.project_total.after.margin_pct, Some(9.4));
    }

    #[test]
    fn validation_rules() {
        assert!(validate_terms(&terms(700_000, 140, 180), "受注").is_ok());
        assert!(validate_terms(&terms(0, 140, 180), "受注").is_err(), "単価0");
        assert!(validate_terms(&terms(700_000, 180, 140), "受注").is_err(), "下限>上限");
        let mut t = terms(700_000, 140, 180);
        t.effort = Decimal::ZERO;
        assert!(validate_terms(&t, "受注").is_err(), "工数0");
        let d = |m, day| NaiveDate::from_ymd_opt(2026, m, day).unwrap();
        assert!(validate_period(d(4, 1), d(4, 1), "受注契約").is_ok(), "同日はOK");
        assert!(validate_period(d(4, 2), d(4, 1), "受注契約").is_err());
    }

    #[test]
    fn project_status_compares_with_the_flow_target() {
        // 売上100万・原価90万 = 粗利率10%
        assert_eq!(project_target_status(Some("SUBCONTRACT"), 1_000_000, 900_000, (20, 8)), (Some(10.0), Some(8), Some(2.0), "OK"));
        assert_eq!(project_target_status(Some("DIRECT"), 1_000_000, 900_000, (20, 8)), (Some(10.0), Some(20), Some(-10.0), "BELOW"));
        assert_eq!(project_target_status(None, 1_000_000, 900_000, (20, 8)), (Some(10.0), None, None, "UNSET"));
        assert_eq!(project_target_status(Some("DIRECT"), 0, 0, (20, 8)), (None, None, None, "NOT_APPLICABLE"), "売上なし");
    }

    #[test]
    fn yen_formats_with_commas_and_sign() {
        assert_eq!(yen(1_234_567), "1,234,567");
        assert_eq!(yen(-20_000), "-20,000");
        assert_eq!(yen(999), "999");
    }
}
