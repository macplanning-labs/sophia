//! 月別状況(UI刷新 3-8 / DEMO-000163)。アサイン(要員×案件)ごとに、受注書から支払通知までの進み具合と、
//! いまできる次の操作を1つだけ返す。月次確定の集計(`SettlementRow`)を元にした純粋関数で、DB は読まない。
//! この画面からは何も更新しない(見る+移動のみ)。

use serde::Serialize;

use crate::domain::services::settlement_dashboard::SettlementRow;

/// セルの状態。色の意味: ok=完了 / warn=進行中・待ち / bad=止まっている / na=対象外
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Ok,
    Warn,
    Bad,
    Na,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Cell {
    pub tone: Tone,
    pub label: String,
}

fn cell(tone: Tone, label: &str) -> Cell {
    Cell { tone, label: label.to_string() }
}

/// 次の操作(移動先はフロントが決める)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NextAction {
    RegisterOrder,
    ReviewTimesheet,
    RerequestTimesheet,
    SendPurchaseOrder,
    ReviewInvoice,
    CreateNotice,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusRow {
    pub engineer_id: i64,
    pub engineer_name: String,
    pub client_id: i64,
    pub client_name: String,
    pub project_id: String,
    pub project_name: String,
    pub received_order: Cell,
    pub purchase_order: Cell,
    pub timesheet: Cell,
    pub invoice: Cell,
    pub notice: Cell,
    pub next_action: Option<NextAction>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct StatusSummary {
    pub total: usize,
    pub needs_action: usize,
    pub invoice_unconfirmed: usize,
    pub timesheet_stalled: usize,
}

fn purchase_order_cell(r: &SettlementRow) -> Cell {
    if r.partner_contract_id.is_none() {
        return cell(Tone::Na, "自社社員");
    }
    match r.purchase_order_status.as_deref() {
        None => cell(Tone::Warn, "未作成"),
        Some("DRAFT") => cell(Tone::Warn, "未送付"),
        Some("SENT") => cell(Tone::Ok, "送付済"),
        Some("REJECTED") | Some("CANCELLED") => cell(Tone::Bad, "却下・取消"),
        Some(_) => cell(Tone::Ok, "承諾済"),
    }
}

fn timesheet_cell(r: &SettlementRow) -> Cell {
    match r.timesheet_status.as_deref() {
        Some("APPROVED") => cell(Tone::Ok, "承認済"),
        Some("UPLOADED") | Some("PARSED") => cell(Tone::Warn, "確認待ち"),
        Some("PENDING") => cell(Tone::Bad, "差し戻し"),
        Some(_) => cell(Tone::Warn, "確認待ち"),
        None if r.received_order_id.is_some() => cell(Tone::Warn, "未提出"),
        None => cell(Tone::Na, "—"),
    }
}

/// 1行分の状態と次の操作。優先順位: 受注書 → 勤務表(確認・差し戻し) → 発注書 → 請求書 → 支払通知
pub fn build_row(r: &SettlementRow) -> StatusRow {
    let has_order = r.received_order_id.is_some();
    let approved = r.timesheet_status.as_deref() == Some("APPROVED");

    let received_order = if has_order { cell(Tone::Ok, "受注登録") } else { cell(Tone::Bad, "注文書なし") };
    let purchase_order = purchase_order_cell(r);
    let timesheet = timesheet_cell(r);

    let invoice = if r.client_invoice_by_client {
        cell(Tone::Na, "先方で作成")
    } else if r.invoice_issued == Some(true) {
        cell(Tone::Ok, "発行済")
    } else if approved && has_order {
        cell(Tone::Warn, "未確定")
    } else {
        cell(Tone::Na, "—")
    };

    let notice = if r.partner_contract_id.is_none() {
        cell(Tone::Na, "自社社員")
    } else if r.notice_issued == Some(true) {
        cell(Tone::Ok, "作成済")
    } else if approved {
        cell(Tone::Warn, "未作成")
    } else {
        cell(Tone::Na, "—")
    };

    let next_action = if !has_order {
        Some(NextAction::RegisterOrder)
    } else if matches!(r.timesheet_status.as_deref(), Some("UPLOADED") | Some("PARSED")) {
        Some(NextAction::ReviewTimesheet)
    } else if r.timesheet_status.as_deref() == Some("PENDING") {
        Some(NextAction::RerequestTimesheet)
    } else if purchase_order.tone == Tone::Warn && purchase_order.label == "未送付" {
        Some(NextAction::SendPurchaseOrder)
    } else if invoice.label == "未確定" {
        Some(NextAction::ReviewInvoice)
    } else if notice.label == "未作成" {
        Some(NextAction::CreateNotice)
    } else {
        None
    };

    StatusRow {
        engineer_id: r.engineer_id,
        engineer_name: r.engineer_name.clone(),
        client_id: r.client_id,
        client_name: r.client_name.clone(),
        project_id: r.project_id.clone(),
        project_name: r.project_name.clone(),
        received_order,
        purchase_order,
        timesheet,
        invoice,
        notice,
        next_action,
    }
}

pub fn build_matrix(rows: &[SettlementRow]) -> (Vec<StatusRow>, StatusSummary) {
    let out: Vec<StatusRow> = rows.iter().map(build_row).collect();
    let summary = StatusSummary {
        total: out.len(),
        needs_action: out.iter().filter(|r| r.next_action.is_some()).count(),
        invoice_unconfirmed: out.iter().filter(|r| r.invoice.label == "未確定").count(),
        timesheet_stalled: out
            .iter()
            .filter(|r| matches!(r.timesheet.label.as_str(), "未提出" | "差し戻し"))
            .count(),
    };
    (out, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn row() -> SettlementRow {
        SettlementRow {
            engineer_id: 1, engineer_name: "山田 太郎".into(), partner_id: None, partner_name: None,
            client_name: "テスト請求先".into(), client_id: 4, client_edi_system_type: "".into(), client_invoice_by_client: false,
            timesheet_id: Some(1), total_hours: Some(Decimal::from(160)), timesheet_status: Some("APPROVED".into()),
            client_contract_id: 1, received_order_id: Some(1), billing_base_rate: 0, billing_settlement_type: "RANGE".into(),
            billing_lower_limit: Decimal::from(140), billing_upper_limit: Decimal::from(180), billing_fixed_hours: None,
            billing_deduction_rate: 0, billing_overtime_rate: 0, billing_effort: Decimal::ONE, billing_payment_terms: "".into(),
            partner_contract_id: Some(1), payment_base_rate: None, payment_settlement_type: None, payment_lower_limit: None,
            payment_upper_limit: None, payment_fixed_hours: None, payment_deduction_rate: None, payment_overtime_rate: None,
            payment_effort: None, payment_condition: "".into(), invoice_issued: Some(false), notice_issued: Some(false),
            purchase_order_id: Some("PO1".into()), purchase_order_status: Some("ACCEPTED".into()),
            project_id: "P1".into(), project_name: "基盤".into(), engineer_employee_code: "".into(),
        }
    }

    #[test]
    fn approved_timesheet_with_unconfirmed_invoice_suggests_reviewing_invoice() {
        let r = build_row(&row());
        assert_eq!(r.invoice, cell(Tone::Warn, "未確定"));
        assert_eq!(r.next_action, Some(NextAction::ReviewInvoice));
    }

    #[test]
    fn no_order_blocks_everything_and_suggests_registering_it() {
        let mut x = row();
        x.received_order_id = None;
        x.timesheet_status = None;
        let r = build_row(&x);
        assert_eq!(r.received_order, cell(Tone::Bad, "注文書なし"));
        assert_eq!(r.timesheet, cell(Tone::Na, "—"));
        assert_eq!(r.next_action, Some(NextAction::RegisterOrder));
    }

    #[test]
    fn returned_timesheet_is_red_and_asks_to_rerequest() {
        let mut x = row();
        x.timesheet_status = Some("PENDING".into());
        let r = build_row(&x);
        assert_eq!(r.timesheet, cell(Tone::Bad, "差し戻し"));
        assert_eq!(r.next_action, Some(NextAction::RerequestTimesheet));
    }

    #[test]
    fn missing_timesheet_is_waiting_not_actionable_unless_other_step_is() {
        let mut x = row();
        x.timesheet_status = None;
        x.purchase_order_status = Some("DRAFT".into());
        let r = build_row(&x);
        assert_eq!(r.timesheet, cell(Tone::Warn, "未提出"));
        assert_eq!(r.next_action, Some(NextAction::SendPurchaseOrder));
    }

    #[test]
    fn employee_has_no_purchase_order_or_notice() {
        let mut x = row();
        x.partner_contract_id = None;
        x.invoice_issued = Some(true);
        let r = build_row(&x);
        assert_eq!(r.purchase_order, cell(Tone::Na, "自社社員"));
        assert_eq!(r.notice, cell(Tone::Na, "自社社員"));
        assert_eq!(r.next_action, None);
    }

    #[test]
    fn client_made_invoice_is_not_ours_and_never_suggested() {
        let mut x = row();
        x.client_invoice_by_client = true;
        x.notice_issued = Some(true);
        let r = build_row(&x);
        assert_eq!(r.invoice, cell(Tone::Na, "先方で作成"));
        assert_eq!(r.next_action, None);
    }

    #[test]
    fn summary_counts() {
        let mut a = row();
        a.timesheet_status = Some("PENDING".into());
        let (rows, s) = build_matrix(&[row(), a]);
        assert_eq!(rows.len(), 2);
        assert_eq!(s, StatusSummary { total: 2, needs_action: 2, invoice_unconfirmed: 1, timesheet_stalled: 1 });
    }
}
