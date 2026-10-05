//! 請求書のプレビュー計算(UI刷新 2-4 / DEMO-000136)。
//!
//! 承認済みの勤務表から、請求書の中身(明細・合計・あと何名・確定できるか)を**表示のたびに計算する**。
//! DBには何も書かない。確定(`billing_confirm`)は、同じ計算をロックの中で再実行してから請求書を作る。
//! 金額・税・集約キー・件名は、旧画面の一括発行(`create_invoices_by_client`)と同じ関数を使う(ずれないように)。

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::domain::services::settlement_dashboard::{
    invoice_group_key, invoice_subject, invoice_totals, BillingUnit, SettlementViewRow,
};

/// 請求書に入る明細(承認済みの勤務表)
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PreviewItem {
    pub client_contract_id: i64,
    pub engineer_id: i64,
    pub engineer_name: String,
    pub project_id: String,
    pub project_name: String,
    pub timesheet_id: Option<i64>,
    pub amount: i32,
}

/// まだ請求書に入れられない契約(勤務表が承認されていない)
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MissingItem {
    pub client_contract_id: i64,
    pub engineer_name: String,
    pub project_id: String,
    pub project_name: String,
    pub timesheet_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InvoicePreview {
    /// 画面の行ID。案件ごと=`c{取引先}:p{案件}`、取引先まとめ=`c{取引先}`
    pub key: String,
    pub client_id: i64,
    pub client_name: String,
    /// 取引先の請求単位("PROJECT" / "CLIENT")
    pub billing_unit: String,
    pub project_id: Option<String>,
    pub subject: String,
    pub items: Vec<PreviewItem>,
    pub subtotal: i32,
    pub tax_amount: i32,
    pub total: i32,
    pub missing: Vec<MissingItem>,
    /// 同じ範囲で、すでに請求書に入っている要員の数(当月2通目の目安)
    pub already_issued_count: usize,
    /// 明細があり、揃っていない要員がおらず、EDI対象外
    pub confirmable: bool,
    /// 請求書を先方が作る取引先（取引先マスタ `invoice_issued_by_client`）は、先方の EDI 経由で受け取るため、Sophia では作らない
    pub edi_excluded: bool,
}

/// 画面の行ID
pub fn preview_key(unit: BillingUnit, client_id: i64, project_id: &str) -> String {
    match invoice_group_key(unit, client_id, project_id) {
        (c, Some(p)) => format!("c{c}:p{p}"),
        (c, None) => format!("c{c}"),
    }
}

/// 月次確定の行から、請求書ごとのプレビューを組み立てる。
/// 対象は「その月に注文書(受注)がある契約」。注文書が無い契約は請求書を作れないので、数えない。
pub fn build_previews(
    view_rows: &[SettlementViewRow],
    units: &HashMap<i64, BillingUnit>,
    tax_rate: Decimal,
    month: NaiveDate,
) -> Vec<InvoicePreview> {
    // 並びを一定にするため BTreeMap(キー順)
    let mut groups: BTreeMap<String, Vec<&SettlementViewRow>> = BTreeMap::new();
    for vr in view_rows {
        if vr.row.received_order_id.is_none() {
            continue;
        }
        let unit = units.get(&vr.row.client_id).copied().unwrap_or(BillingUnit::Project);
        groups
            .entry(preview_key(unit, vr.row.client_id, &vr.row.project_id))
            .or_default()
            .push(vr);
    }

    let mut out = Vec::with_capacity(groups.len());
    for (key, rows) in groups {
        let first = rows[0];
        let client_id = first.row.client_id;
        let unit = units.get(&client_id).copied().unwrap_or(BillingUnit::Project);
        let edi_excluded = first.row.client_invoice_by_client;

        let mut items = Vec::new();
        let mut missing = Vec::new();
        let mut already_issued_count = 0usize;
        for vr in &rows {
            if vr.row.invoice_issued == Some(true) {
                already_issued_count += 1;
            } else if vr.row.timesheet_status.as_deref() == Some("APPROVED") {
                items.push(PreviewItem {
                    client_contract_id: vr.row.client_contract_id,
                    engineer_id: vr.row.engineer_id,
                    engineer_name: vr.row.engineer_name.clone(),
                    project_id: vr.row.project_id.clone(),
                    project_name: vr.row.project_name.clone(),
                    timesheet_id: vr.row.timesheet_id,
                    amount: vr.billing_amount,
                });
            } else {
                missing.push(MissingItem {
                    client_contract_id: vr.row.client_contract_id,
                    engineer_name: vr.row.engineer_name.clone(),
                    project_id: vr.row.project_id.clone(),
                    project_name: vr.row.project_name.clone(),
                    timesheet_status: vr.row.timesheet_status.clone(),
                });
            }
        }

        let subtotal: i32 = items.iter().map(|i| i.amount).sum();
        let (tax_amount, total) = invoice_totals(tax_rate, subtotal);
        let project_id = match unit {
            BillingUnit::Project => Some(first.row.project_id.clone()),
            BillingUnit::Client => None,
        };
        let subject = invoice_subject(unit, &first.row.project_name, &first.row.client_name, month);
        let confirmable = !edi_excluded && !items.is_empty() && missing.is_empty();

        out.push(InvoicePreview {
            key,
            client_id,
            client_name: first.row.client_name.clone(),
            billing_unit: unit.as_str().to_string(),
            project_id,
            subject,
            items,
            subtotal,
            tax_amount,
            total,
            missing,
            already_issued_count,
            confirmable,
            edi_excluded,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::settlement_dashboard::SettlementRow;

    fn row(contract: i64, client: i64, project: &str, status: Option<&str>, ro: Option<i64>, issued: bool, invoice_by_client: bool) -> SettlementViewRow {
        SettlementViewRow {
            row: SettlementRow {
                engineer_id: contract,
                engineer_name: format!("要員{contract}"),
                partner_id: None,
                partner_name: None,
                client_name: format!("取引先{client}"),
                client_id: client,
                client_edi_system_type: String::new(),
                client_invoice_by_client: invoice_by_client,
                timesheet_id: Some(contract * 10),
                total_hours: None,
                timesheet_status: status.map(|s| s.to_string()),
                client_contract_id: contract,
                received_order_id: ro,
                billing_base_rate: 0,
                billing_settlement_type: String::new(),
                billing_lower_limit: Decimal::ZERO,
                billing_upper_limit: Decimal::ZERO,
                billing_fixed_hours: None,
                billing_deduction_rate: 0,
                billing_overtime_rate: 0,
                billing_effort: Decimal::ZERO,
                billing_payment_terms: String::new(),
                partner_contract_id: None,
                payment_base_rate: None,
                payment_settlement_type: None,
                payment_lower_limit: None,
                payment_upper_limit: None,
                payment_fixed_hours: None,
                payment_deduction_rate: None,
                payment_overtime_rate: None,
                payment_effort: None,
                payment_condition: String::new(),
                invoice_issued: Some(issued),
                notice_issued: None,
                purchase_order_id: None,
                purchase_order_status: None,
                project_id: project.to_string(),
                project_name: format!("案件{project}"),
                engineer_employee_code: String::new(),
            },
            billing_amount: 100_000,
            payment_amount: 0,
            profit: 0,
            profit_rate: 0.0,
            profit_rate_display: String::new(),
        }
    }

    fn month() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
    }

    fn rate10() -> Decimal {
        Decimal::from(10)
    }

    #[test]
    fn all_approved_is_confirmable_with_totals() {
        let rows = vec![
            row(1, 7, "P1", Some("APPROVED"), Some(1), false, false),
            row(2, 7, "P1", Some("APPROVED"), Some(2), false, false),
        ];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].key, "c7:pP1");
        assert!(p[0].confirmable);
        assert_eq!(p[0].subtotal, 200_000);
        assert_eq!(p[0].tax_amount, 20_000);
        assert_eq!(p[0].total, 220_000);
        assert_eq!(p[0].subject, "案件P1 2026年10月分 請求書");
        assert_eq!(p[0].project_id.as_deref(), Some("P1"));
    }

    #[test]
    fn one_unapproved_blocks_confirmation_and_is_listed() {
        let rows = vec![
            row(1, 7, "P1", Some("APPROVED"), Some(1), false, false),
            row(2, 7, "P1", Some("UPLOADED"), Some(2), false, false),
        ];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert!(!p[0].confirmable);
        assert_eq!(p[0].items.len(), 1);
        assert_eq!(p[0].missing.len(), 1);
        assert_eq!(p[0].missing[0].client_contract_id, 2);
        assert_eq!(p[0].missing[0].timesheet_status.as_deref(), Some("UPLOADED"));
    }

    #[test]
    fn rows_without_received_order_are_out_of_scope() {
        let rows = vec![
            row(1, 7, "P1", Some("APPROVED"), Some(1), false, false),
            row(2, 7, "P1", None, None, false, false), // 注文書なし: 数えない(「あと1名」にしない)
        ];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert!(p[0].confirmable);
        assert!(p[0].missing.is_empty());
    }

    #[test]
    fn already_issued_rows_are_not_billed_twice_and_support_second_invoice() {
        // 1人は発行済み。後から承認された1人だけが、2通目の明細になる(現行の運用)
        let rows = vec![
            row(1, 7, "P1", Some("APPROVED"), Some(1), true, false),
            row(2, 7, "P1", Some("APPROVED"), Some(2), false, false),
        ];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert_eq!(p[0].already_issued_count, 1);
        assert_eq!(p[0].items.len(), 1);
        assert_eq!(p[0].items[0].client_contract_id, 2);
        assert!(p[0].confirmable);
    }

    #[test]
    fn nothing_to_bill_is_not_confirmable() {
        let rows = vec![row(1, 7, "P1", Some("APPROVED"), Some(1), true, false)];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert!(p[0].items.is_empty());
        assert!(!p[0].confirmable, "明細が無ければ確定できない(発行済みのみ)");
    }

    #[test]
    fn clients_issuing_their_own_invoice_are_excluded() {
        let rows = vec![row(1, 7, "P1", Some("APPROVED"), Some(1), false, true)];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert!(p[0].edi_excluded);
        assert!(!p[0].confirmable);
    }

    #[test]
    fn client_unit_merges_projects_into_one_invoice() {
        let rows = vec![
            row(1, 7, "P1", Some("APPROVED"), Some(1), false, false),
            row(2, 7, "P2", Some("APPROVED"), Some(2), false, false),
            row(3, 8, "P3", Some("APPROVED"), Some(3), false, false),
        ];
        let units = HashMap::from([(7, BillingUnit::Client)]);
        let p = build_previews(&rows, &units, rate10(), month());
        assert_eq!(p.len(), 2, "取引先7は1通、取引先8(案件ごと)は1通");
        let a = p.iter().find(|x| x.client_id == 7).unwrap();
        assert_eq!(a.key, "c7");
        assert_eq!(a.items.len(), 2);
        assert_eq!(a.project_id, None);
        assert_eq!(a.billing_unit, "CLIENT");
        assert_eq!(a.subject, "取引先7 2026年10月分 請求書");
        let b = p.iter().find(|x| x.client_id == 8).unwrap();
        assert_eq!(b.key, "c8:pP3");
    }

    #[test]
    fn project_unit_splits_by_project() {
        let rows = vec![
            row(1, 7, "P1", Some("APPROVED"), Some(1), false, false),
            row(2, 7, "P2", Some("UPLOADED"), Some(2), false, false),
        ];
        let p = build_previews(&rows, &HashMap::new(), rate10(), month());
        assert_eq!(p.len(), 2);
        assert!(p.iter().find(|x| x.key == "c7:pP1").unwrap().confirmable);
        assert!(!p.iter().find(|x| x.key == "c7:pP2").unwrap().confirmable, "別案件の未承認は、P1 の確定を止めない");
    }
}
