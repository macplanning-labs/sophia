//! domain/authz/redact.rs — 機微な列を出力から落とす(DEMO-000173 PR 8。設計書 §8.4)。
//!
//! 入口の権限は通っても、「この列は見せない」を列の単位で決めたい場面で使う。
//! - 社員 API: 人事は全社員を見られるが、給与の金額は見せない(U8)。
//! - プロジェクト別ダッシュボード: `contract.price:view` の無い人に、単価・売上・仕入・利益を見せない(D2)。
//!
//! 画面が壊れないよう、列は消さずに `null` にする(形は変えない)。DB には触れない純粋な関数。

use serde_json::Value;

/// 社員 API で、給与の金額にあたる列(基本給・各種手当・標準報酬月額)。
pub const EMPLOYEE_SALARY_KEYS: &[&str] = &[
    "base_salary",
    "position_allowance",
    "housing_allowance",
    "commuting_allowance",
    "standard_remuneration",
];

/// 単価・売上・仕入・利益の金額にあたる列(プロジェクト別ダッシュボードと、その中の要員の行)。
pub const CONTRACT_PRICE_KEYS: &[&str] = &[
    // プロジェクトごとの集計
    "billing_total",
    "partner_cost_total",
    "internal_cost_total",
    "profit",
    // 要員の行(売上・仕入の金額と、その元になる単価・精算の率)
    "billing_amount",
    "payment_amount",
    "profit_rate",
    "profit_rate_display",
    "billing_base_rate",
    "billing_deduction_rate",
    "billing_overtime_rate",
    "payment_base_rate",
    "payment_deduction_rate",
    "payment_overtime_rate",
];

/// `value` の中(入れ子も含む)にある、`keys` に挙げた名前の列を `null` にする。
pub fn redact_keys(value: &mut Value, keys: &[&str]) {
    match value {
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if keys.contains(&k.as_str()) {
                    *v = Value::Null;
                } else {
                    redact_keys(v, keys);
                }
            }
        }
        Value::Array(items) => {
            for v in items {
                redact_keys(v, keys);
            }
        }
        _ => {}
    }
}

/// 社員情報から、給与の金額を落とす。
pub fn redact_employee_salary(value: &mut Value) {
    redact_keys(value, EMPLOYEE_SALARY_KEYS);
}

/// 単価・売上・仕入・利益の金額を落とす。
pub fn redact_contract_prices(value: &mut Value) {
    redact_keys(value, CONTRACT_PRICE_KEYS);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn 社員の給与の列だけがnullになる() {
        let mut v = json!({
            "id": 1, "last_name": "山田", "email": "y@example.com",
            "base_salary": 300000, "position_allowance": 1, "housing_allowance": 2,
            "commuting_allowance": 3, "standard_remuneration": 280000,
            "standard_monthly_hours": "160", "dependents_count": 2
        });
        redact_employee_salary(&mut v);
        for k in EMPLOYEE_SALARY_KEYS {
            assert!(v[*k].is_null(), "{k} が残っている");
        }
        assert_eq!(v["last_name"], "山田");
        assert_eq!(v["email"], "y@example.com");
        assert_eq!(v["dependents_count"], 2);
        assert_eq!(v["standard_monthly_hours"], "160");
    }

    #[test]
    fn 一覧の全員分が落ちる() {
        let mut v = json!([{ "id": 1, "base_salary": 1 }, { "id": 2, "base_salary": 2 }]);
        redact_employee_salary(&mut v);
        assert!(v[0]["base_salary"].is_null() && v[1]["base_salary"].is_null());
        assert_eq!(v[1]["id"], 2);
    }

    #[test]
    fn 金額の列は入れ子の中も落ちる() {
        let mut v = json!([{
            "project_name": "A", "billing_total": 100, "partner_cost_total": 50, "internal_cost_total": 10, "profit": 40,
            "engineers": [{ "row": { "engineer_name": "田中", "billing_base_rate": 700000, "payment_base_rate": 600000 },
                            "billing_amount": 700000, "payment_amount": 600000, "profit": 100000,
                            "profit_rate": 14.2, "profit_rate_display": "14.2%" }]
        }]);
        redact_contract_prices(&mut v);
        assert_eq!(v[0]["project_name"], "A");
        assert!(v[0]["billing_total"].is_null() && v[0]["profit"].is_null());
        let e = &v[0]["engineers"][0];
        assert_eq!(e["row"]["engineer_name"], "田中");
        assert!(e["row"]["billing_base_rate"].is_null() && e["row"]["payment_base_rate"].is_null());
        assert!(e["billing_amount"].is_null() && e["profit_rate_display"].is_null());
    }

    #[test]
    fn 対象の列が無ければ何も変わらない() {
        let mut v = json!({ "a": 1, "b": [1, 2, { "c": "x" }] });
        let before = v.clone();
        redact_contract_prices(&mut v);
        assert_eq!(v, before);
    }
}
