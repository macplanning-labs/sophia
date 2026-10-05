/// domain/services/jp_pint_mapper.rs — JP PINT準拠 中間JSONマッピング
///
/// UBL/JP PINTのセマンティック名に準拠した中間表現。実プロバイダーのAPI形式への
/// 変換（フィールド名・ネスト構造の調整）はinfrastructure::peppol_clientで行う想定。

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;

use crate::domain::models::billing::{BillingInvoice, BillingItem};
use crate::domain::models::client::Client;
use crate::domain::models::partner::Partner;
use crate::domain::models::partner_contract::{PaymentNotice, PaymentNoticeItem};
use crate::domain::services::tax_calculation::{calculate_tax_breakdown, TaxBreakdown};
use crate::infrastructure::repositories::company_info_repo::CompanyInfo;

#[derive(Debug, Clone, Serialize)]
pub struct JpPintAddress {
    pub postal_code: String,
    pub address_line: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JpPintParty {
    pub registration_name: String,
    /// 適格請求書発行事業者登録番号（"T"+13桁）
    pub company_id: String,
    pub peppol_participant_id: String,
    pub postal_address: JpPintAddress,
}

#[derive(Debug, Clone, Serialize)]
pub struct JpPintTaxSubtotal {
    pub tax_rate: Decimal,
    pub taxable_amount: i32,
    pub tax_amount: i32,
}

impl From<&TaxBreakdown> for JpPintTaxSubtotal {
    fn from(b: &TaxBreakdown) -> Self {
        Self { tax_rate: b.rate, taxable_amount: b.taxable_amount, tax_amount: b.tax_amount }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct JpPintInvoiceLine {
    pub id: i32,
    pub item_name: String,
    pub quantity: Decimal,
    pub unit_price: i32,
    pub line_amount: i32,
    pub tax_rate: Decimal,
    /// SES精算条件（基準時間/上下限時間/清算単価/超過控除単価）を整形したフリーテキスト
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct JpPintInvoice {
    pub invoice_type: &'static str,
    pub invoice_id: String,
    pub issue_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub seller: JpPintParty,
    pub buyer: JpPintParty,
    pub lines: Vec<JpPintInvoiceLine>,
    pub tax_subtotals: Vec<JpPintTaxSubtotal>,
    pub tax_total: i32,
    pub payable_amount: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct JpPintSelfBillingInvoice {
    pub invoice_type: &'static str,
    pub notice_id: String,
    pub issue_date: NaiveDate,
    /// セルフビリングのため buyer/seller が売上請求書と逆転する
    /// （seller=パートナー、buyer=自社）
    pub seller: JpPintParty,
    pub buyer: JpPintParty,
    pub lines: Vec<JpPintInvoiceLine>,
    pub tax_subtotals: Vec<JpPintTaxSubtotal>,
    pub tax_total: i32,
    pub payable_amount: i32,
}

/// SES精算条件をLine Note用テキストに整形する
fn format_settlement_note(
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    base_rate: i32,
    deduction_rate: i32,
    overtime_rate: i32,
    actual_hours: Decimal,
) -> String {
    format!(
        "基準時間: {lower_limit_hours}h〜{upper_limit_hours}h / 清算単価: {base_rate}円 / \
         控除単価: {deduction_rate}円/h / 超過単価: {overtime_rate}円/h / 実稼働: {actual_hours}h"
    )
}

fn to_party(registration_name: &str, company_id: &str, peppol_participant_id: &str, postal_code: &str, address_line: &str) -> JpPintParty {
    JpPintParty {
        registration_name: registration_name.to_string(),
        company_id: company_id.to_string(),
        peppol_participant_id: peppol_participant_id.to_string(),
        postal_address: JpPintAddress {
            postal_code: postal_code.to_string(),
            address_line: address_line.to_string(),
        },
    }
}

/// 売上請求書（自社→クライアント）をJP PINT形式にマッピングする
pub fn map_sales_invoice(
    invoice: &BillingInvoice,
    items: &[BillingItem],
    client: &Client,
    company: &CompanyInfo,
    own_participant_id: &str,
) -> JpPintInvoice {
    let lines: Vec<JpPintInvoiceLine> = items
        .iter()
        .enumerate()
        .map(|(i, item)| JpPintInvoiceLine {
            id: i as i32 + 1,
            item_name: item.description.clone(),
            quantity: item.quantity,
            unit_price: item.unit_price,
            line_amount: item.amount,
            tax_rate: item.tax_rate,
            note: format_settlement_note(
                item.lower_limit,
                item.upper_limit,
                item.unit_price,
                item.deduction_rate,
                item.overtime_rate,
                item.quantity,
            ),
        })
        .collect();

    let breakdown = calculate_tax_breakdown(&items.iter().map(|i| (i.tax_rate, i.amount)).collect::<Vec<_>>());
    let tax_subtotals: Vec<JpPintTaxSubtotal> = breakdown.iter().map(JpPintTaxSubtotal::from).collect();
    let tax_total: i32 = breakdown.iter().map(|b| b.tax_amount).sum();

    JpPintInvoice {
        invoice_type: "INVOICE",
        invoice_id: invoice.invoice_no.clone(),
        issue_date: invoice.issue_date,
        due_date: invoice.due_date,
        seller: to_party(&company.name, &company.registration_no, own_participant_id, &company.postal_code, &company.address),
        buyer: to_party(&client.name, "", &client.peppol_participant_id, "", &client.address),
        lines,
        tax_subtotals,
        tax_total,
        payable_amount: invoice.total,
    }
}

/// 支払通知書（セルフビリング／自社→パートナーへの代理請求書）をJP PINT形式にマッピングする。
/// `item_names` は明細ごとのエンジニア名（呼び出し側でm_partner_contract経由の解決結果を渡す）。
pub fn map_self_billing_notice(
    notice: &PaymentNotice,
    items: &[PaymentNoticeItem],
    item_names: &[String],
    partner: &Partner,
    company: &CompanyInfo,
    own_participant_id: &str,
) -> JpPintSelfBillingInvoice {
    let lines: Vec<JpPintInvoiceLine> = items
        .iter()
        .enumerate()
        .map(|(i, item)| JpPintInvoiceLine {
            id: i as i32 + 1,
            item_name: item_names.get(i).cloned().unwrap_or_default(),
            quantity: item.actual_hours,
            unit_price: item.base_fee,
            line_amount: item.amount,
            tax_rate: item.tax_rate,
            note: format_settlement_note(
                item.lower_limit_hours,
                item.upper_limit_hours,
                item.base_fee,
                item.deduction_rate,
                item.overtime_rate,
                item.actual_hours,
            ),
        })
        .collect();

    let breakdown = calculate_tax_breakdown(&items.iter().map(|i| (i.tax_rate, i.amount)).collect::<Vec<_>>());
    let tax_subtotals: Vec<JpPintTaxSubtotal> = breakdown.iter().map(JpPintTaxSubtotal::from).collect();
    let tax_total: i32 = breakdown.iter().map(|b| b.tax_amount).sum();

    JpPintSelfBillingInvoice {
        invoice_type: "SELF_BILLED_INVOICE",
        notice_id: notice.notice_id.clone(),
        issue_date: notice.notice_date,
        seller: to_party(&partner.name, &partner.registration_no, &partner.peppol_participant_id, &partner.postal_code, &partner.address),
        buyer: to_party(&company.name, &company.registration_no, own_participant_id, &company.postal_code, &company.address),
        lines,
        tax_subtotals,
        tax_total,
        payable_amount: notice.total,
    }
}
