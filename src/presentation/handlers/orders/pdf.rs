/// orders/pdf.rs — 発注書PDFデータ構築（共通ロジック）

use sqlx::PgPool;

use crate::domain::models::partner_contract::PurchaseOrder;
use crate::infrastructure::repositories::order_repo;

/// 注文書PDFデータ構築（共通関数）
///
/// orders/ / token.rs の全PDFハンドラから呼ばれる唯一のデータ構築ロジック。
/// フィールド追加時はここだけ修正すればOK。
pub async fn build_purchase_order_pdf_data(
    pool: &PgPool,
    order: &PurchaseOrder,
) -> anyhow::Result<crate::domain::services::pdf_generator::PurchaseOrderPdfData> {
    use crate::domain::services::pdf_generator::{PurchaseOrderPdfData, OrderPdfItem};

    let (partner_name, partner_address, partner_tel) = order_repo::find_partner_name_address_tel(pool, &order.partner_id).await
        .unwrap_or_else(|e| { tracing::warn!("build_pdf: {:?}", e); None })
        .unwrap_or_default();

    let project_name = order_repo::find_project_name(pool, &order.project_id).await
        .unwrap_or_else(|e| { tracing::warn!("build_pdf: {:?}", e); None })
        .unwrap_or_default();

    let items = order_repo::list_order_items_ordered(pool, &order.order_id).await
        .unwrap_or_else(|e| { tracing::warn!("build_pdf: {:?}", e); vec![] });

    // 技術者名・テンプレート情報を発注契約から取得
    let mut engineer_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    let mut work_location_for_pdf: Option<String> = if !order.work_location.is_empty() {
        Some(order.work_location.clone())
    } else {
        None
    };
    // テンプレート情報（最初の発注契約から取得）
    let mut kou_responsible: Option<String> = None;
    let mut kou_contact: Option<String> = None;
    let mut otsu_responsible: Option<String> = None;
    let mut otsu_contact: Option<String> = None;
    let mut work_responsible_name: Option<String> = None;
    let mut deliverable_text: Option<String> = None;
    let mut payment_condition_val: Option<String> = None;

    for item in &items {
        // 技術者名・作業場所
        let basic = order_repo::find_contract_engineer_and_workplace(pool, item.partner_contract_id).await
            .unwrap_or_else(|e| { tracing::warn!("pdf basic query: {:?}", e); None });

        if let Some((n, wl)) = basic {
            engineer_names.insert(item.partner_contract_id, n);
            if !wl.is_empty() && work_location_for_pdf.is_none() {
                work_location_for_pdf = Some(wl);
            }
        }

        // テンプレート情報（最初の契約からのみ取得）
        if kou_responsible.is_none() {
            let tmpl = order_repo::find_contract_pdf_template_info(pool, item.partner_contract_id).await
                .unwrap_or_else(|e| { tracing::warn!("pdf template query: {:?}", e); None });

            if let Some((kr, kc, or, oc, wr, dt, pc_cond)) = tmpl {
                if !kr.is_empty() { kou_responsible = Some(kr); }
                if !kc.is_empty() { kou_contact = Some(kc); }
                if !or.is_empty() { otsu_responsible = Some(or); }
                if !oc.is_empty() { otsu_contact = Some(oc); }
                if !wr.is_empty() { work_responsible_name = Some(wr); }
                if !dt.is_empty() { deliverable_text = Some(dt); }
                if !pc_cond.is_empty() { payment_condition_val = Some(pc_cond); }
            }
        }
    }
    // 作業責任者: テンプレート情報 > 技術者名
    let work_responsible = work_responsible_name.or_else(|| {
        items.first()
            .and_then(|i| engineer_names.get(&i.partner_contract_id))
            .cloned()
    });

    let company = order_repo::find_company_info(pool).await.ok().flatten();
    let (company_name, company_addr, company_tel, rep_name) = company.unwrap_or_default();

    let total: i64 = items.iter().map(|i| i.amount as i64).sum();

    Ok(PurchaseOrderPdfData {
        order_id: order.order_id.clone(),
        order_date: order.order_date,
        partner_name,
        project_name,
        work_start: order.work_start,
        work_end: order.work_end,
        items: items.iter().map(|item| OrderPdfItem {
            engineer_name: engineer_names.get(&item.partner_contract_id)
                .cloned().unwrap_or_else(|| format!("契約#{}", item.partner_contract_id)),
            unit_price: item.base_fee as i64,
            man_month: item.effort.to_string(),
            amount: item.amount as i64,
            base_fee: Some(item.base_fee as i64),
            deduction_rate: Some(item.deduction_rate as i64),
            overtime_rate: Some(item.overtime_rate as i64),
            lower_limit_hours: Some(item.lower_limit_hours.to_string().parse::<f64>().unwrap_or(0.0)),
            upper_limit_hours: Some(item.upper_limit_hours.to_string().parse::<f64>().unwrap_or(0.0)),
        }).collect(),
        total,
        company_name,
        company_address: company_addr,
        company_tel,
        representative_name: rep_name,
        work_responsible,
        workplace: work_location_for_pdf,
        kou_responsible,
        kou_contact,
        otsu_responsible,
        otsu_contact,
        deliverable_text,
        payment_condition: payment_condition_val,
        partner_address: Some(partner_address),
        partner_tel: Some(partner_tel),
        ..Default::default()
    })
}
