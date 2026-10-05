/// home/edi.rs — 取引先 EDI の通知メールの処理（手動）と、受注書・請求書の PDF 生成
///
/// 取引先の EDI へ接続するかは、メール取込の中で、取引先ごとのカスタマイズ（`crate::custom`）が
/// 届いた通知メールの内容から決める。画面のボタンは、届いている通知メールの処理（自動取込と同じ処理）だけを行う。
/// 画面のボタンから取引先の EDI に接続して、一覧の確認・取込・承認をする API は持たない（DEMO-000148）。

use axum::{
    extract::{State, Json},
    http::StatusCode,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use super::StatusResponse;

/// POST /api/edi/import — 届いている取引先 EDI の通知メールを処理する
///
/// mail_pipeline::run_pipeline（Phase1〜4）を実行する。
pub async fn edi_import(
    State(pool): State<PgPool>,
) -> (StatusCode, Json<StatusResponse>) {
    let result = crate::infrastructure::mail_pipeline::run_pipeline(&pool).await;
    let has_errors = result.has_errors();
    let msg = result.to_string();
    (StatusCode::OK, Json(StatusResponse {
        ok: !has_errors,
        error: Some(msg),
    }))
}

/// POST /api/edi/import-all — 届いている通知メールの処理 + PDF生成
///
/// 1. 届いている EDI 通知メールを処理する（要対応のメールの書類だけを取る）
/// 2. バックグラウンドでPDF生成 + Google Driveアップロード
///
pub async fn edi_import_all(
    State(pool): State<PgPool>,
) -> (StatusCode, Json<serde_json::Value>) {
    let mut messages: Vec<String> = Vec::new();

    let pipeline_result = crate::infrastructure::mail_pipeline::run_pipeline(&pool).await;
    let has_error = pipeline_result.has_errors();
    if has_error {
        messages.push(format!("取込エラー: {pipeline_result}"));
    } else {
        messages.push(format!("取込: {pipeline_result}"));
    }
    let new_orders = &pipeline_result.phase3.new_orders;
    if new_orders.is_empty() {
        messages.push("新しい注文書はありません".to_string());
    } else {
        messages.push(format!("新規注文書 {}件: {}", new_orders.len(), new_orders.join(" / ")));
    }
    let new_invoice_links = &pipeline_result.phase3.new_invoices;
    if !new_invoice_links.is_empty() {
        messages.push(format!(
            "新規に突合できた支払通知書/請求書 {}件: {}",
            new_invoice_links.len(),
            new_invoice_links.join(" / ")
        ));
    }
    messages.push("取引先 EDI の一覧・承認は、取引先の EDI の画面で行ってください".to_string());

    let bg_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = generate_and_upload_pdfs(&bg_pool).await {
            tracing::error!("[一括取込] PDF生成/Driveアップロードエラー: {}", e);
        }
    });

    (StatusCode::OK, Json(serde_json::json!({
        "ok": !has_error,
        "error": messages.join(" / "),
    })))
}

/// PDF未生成の注文書・請求書に対してPDFを生成し、Google Driveにアップロードする
async fn generate_and_upload_pdfs(pool: &PgPool) -> anyhow::Result<()> {
    use crate::domain::services::pdf_generator::{
        PdfGenerator, PurchaseOrderPdfData, OrderPdfItem,
        InvoicePdfData, InvoicePdfItem,
    };
    use crate::domain::services::drive_service;

    let gen = PdfGenerator::new();

    // ── 注文書PDF（order_pdf が空のもの）──
    let orders_without_pdf = crate::infrastructure::repositories::order_repo::list_purchase_orders_without_pdf(pool)
        .await
        .log_err().unwrap_or_default();

    for order in &orders_without_pdf {
        let partner_name: String = crate::infrastructure::repositories::order_repo::find_partner_name_email(pool, &order.partner_id)
            .await
            .ok()
            .flatten()
            .map(|(name, _email)| name)
            .unwrap_or_default();

        let project_name: String = crate::infrastructure::repositories::order_repo::find_project_name(pool, &order.project_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let items: Vec<(String, i64, String, i64)> = crate::infrastructure::repositories::order_repo::find_order_items_for_pdf(pool, &order.order_id)
            .await
            .log_err().unwrap_or_default();

        let (company_name, company_addr, company_tel, rep_name) = crate::infrastructure::repositories::order_repo::find_company_info(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();

        let total: i64 = items.iter().map(|i| i.3).sum();

        let pdf_data = PurchaseOrderPdfData {
            order_id: order.order_id.clone(),
            order_date: order.order_date,
            partner_name: partner_name.clone(),
            project_name,
            work_start: order.work_start,
            work_end: order.work_end,
            items: items.iter().map(|(name, price, effort, amount)| OrderPdfItem {
                engineer_name: name.clone(),
                unit_price: *price,
                man_month: effort.clone(),
                amount: *amount,
                ..Default::default()
            }).collect(),
            total,
            company_name,
            company_address: company_addr,
            company_tel,
            representative_name: rep_name,
            ..Default::default()
        };

        match gen.generate_purchase_order_pdf(&pdf_data) {
            Ok(pdf_bytes) => {
                // Google Driveアップロード
                match drive_service::upload_order_pdf(&partner_name, &order.order_id, &pdf_bytes).await {
                    Ok((file_id, _web_link)) => {
                        let drive_url = drive_service::get_drive_file_url(&file_id);
                        if let Err(e) = crate::infrastructure::repositories::order_repo::set_purchase_order_pdf(pool, &order.order_id, &drive_url).await {
                            tracing::error!("[PDF] 注文書 {} URL保存失敗: {:?}", order.order_id, e);
                        }
                        tracing::info!("[PDF] 注文書 {} → Drive保存完了", order.order_id);
                    }
                    Err(e) => tracing::warn!("[PDF] 注文書 {} Driveアップロード失敗: {}", order.order_id, e),
                }
            }
            Err(e) => tracing::warn!("[PDF] 注文書 {} PDF生成失敗: {}", order.order_id, e),
        }
    }

    // ── 請求書PDF（invoice_pdf が空のもの）──
    let invoices_without_pdf = crate::infrastructure::repositories::billing_repo::list_invoices_without_pdf(pool)
        .await
        .log_err().unwrap_or_default();

    for inv in &invoices_without_pdf {
        let client_name: String = crate::infrastructure::repositories::client_repo::find_by_id(pool, inv.client_id)
            .await
            .ok()
            .flatten()
            .map(|c| c.name)
            .unwrap_or_default();

        // 明細取得（精算条件付き — 超過/控除をPDFへ渡すためフル行を使う）
        let billing_items = crate::infrastructure::repositories::billing_repo::list_invoice_items(pool, inv.id)
            .await
            .log_err().unwrap_or_default();

        let company = crate::infrastructure::repositories::billing_repo::find_company_invoice_info(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let (company_name, company_addr, company_tel, postal_code, fax, rep_title, rep_name, reg_number) =
            (company.name, company.address, company.tel, company.postal_code, company.fax, company.representative_title, company.representative_name, company.registration_number);
        let (bank_name, bank_branch, account_type, account_number, account_name) =
            (company.bank_name, company.bank_branch, company.account_type, company.account_number, company.account_name);

        let pdf_items: Vec<InvoicePdfItem> = if billing_items.is_empty() {
            vec![InvoicePdfItem {
                product_name: inv.subject.clone(),
                man_month: "1.0".to_string(),
                unit_price: inv.subtotal as i64,
                amount: inv.subtotal as i64,
                ..Default::default()
            }]
        } else {
            billing_items.iter().map(InvoicePdfItem::from_billing_item).collect()
        };

        let pdf_data = InvoicePdfData {
            invoice_id: inv.invoice_no.clone(),
            issue_date: inv.issue_date,
            due_date: inv.due_date,
            client_name: client_name.clone(),
            subject: if inv.subject.is_empty() {
                format!("{}月分 業務委託料", inv.target_month.format("%Y-%m"))
            } else {
                inv.subject.clone()
            },
            items: pdf_items,
            subtotal: inv.subtotal as i64,
            tax_amount: inv.tax_amount as i64,
            total: inv.total as i64,
            notes: String::new(),
            company_name,
            company_postal_code: postal_code,
            company_address: company_addr,
            company_tel,
            company_fax: fax,
            company_representative_title: rep_title,
            company_representative_name: rep_name,
            registration_number: reg_number,
            bank_name,
            bank_branch,
            account_type,
            account_number,
            account_name,
        };

        match gen.generate_invoice_pdf(&pdf_data) {
            Ok(pdf_bytes) => {
                match drive_service::upload_invoice_pdf(&client_name, &inv.invoice_no, &pdf_bytes).await {
                    Ok((file_id, _web_link)) => {
                        let drive_url = drive_service::get_drive_file_url(&file_id);
                        if let Err(e) = crate::infrastructure::repositories::billing_repo::set_invoice_pdf(pool, inv.id, &drive_url, &file_id).await {
                            tracing::error!("[PDF] 請求書 {} URL保存失敗: {:?}", inv.invoice_no, e);
                        }
                        tracing::info!("[PDF] 請求書 {} → Drive保存完了", inv.invoice_no);
                    }
                    Err(e) => tracing::warn!("[PDF] 請求書 {} Driveアップロード失敗: {}", inv.invoice_no, e),
                }
            }
            Err(e) => tracing::warn!("[PDF] 請求書 {} PDF生成失敗: {}", inv.invoice_no, e),
        }
    }

    let order_count = orders_without_pdf.len();
    let invoice_count = invoices_without_pdf.len();
    if order_count > 0 || invoice_count > 0 {
        tracing::info!(
            "[一括取込] PDF生成完了: 注文書{}件, 請求書{}件",
            order_count, invoice_count
        );
    }

    Ok(())
}
