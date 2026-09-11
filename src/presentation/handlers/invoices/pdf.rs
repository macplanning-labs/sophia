/// invoices/pdf.rs — 請求書PDFダウンロード

use axum::extract::{Path, State};
use sqlx::PgPool;

use crate::domain::models::billing::BillingInvoice;
use crate::domain::services::pdf_generator::{InvoicePdfData, InvoicePdfItem, PdfGenerator};
use crate::infrastructure::repositories::billing_repo;
use crate::infrastructure::repositories::order_repo;

use super::get_company_info;

/// クライアント向け請求書PDFデータを構築する（/pdf・メール添付・Drive再生成で共用）
pub async fn build_client_invoice_pdf_data(
    pool: &PgPool,
    invoice: &BillingInvoice,
    client_name: &str,
) -> InvoicePdfData {
    let items = billing_repo::list_invoice_items(pool, invoice.id).await
        .unwrap_or_else(|e| { tracing::warn!("invoices: fetch_all failed: {:?}", e); vec![] });

    let company = get_company_info(pool).await;

    InvoicePdfData {
        invoice_id: invoice.invoice_no.clone(),
        issue_date: invoice.issue_date,
        due_date: invoice.due_date,
        client_name: client_name.to_string(),
        subject: invoice.subject.clone(),
        items: items.iter().map(InvoicePdfItem::from_billing_item).collect(),
        subtotal: invoice.subtotal as i64,
        tax_amount: invoice.tax_amount as i64,
        total: invoice.total as i64,
        notes: invoice.subject.clone(),
        company_name: company.name,
        company_postal_code: company.postal_code,
        company_address: company.address,
        company_tel: company.tel,
        company_fax: company.fax,
        company_representative_title: company.representative_title,
        company_representative_name: company.representative_name,
        registration_number: company.registration_number,
        bank_name: company.bank_name,
        bank_branch: company.bank_branch,
        account_type: company.account_type,
        account_number: company.account_number,
        account_name: company.account_name,
    }
}

/// GET /invoices/{id}/pdf — 請求書PDFダウンロード
pub async fn download_pdf(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl axum::response::IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};

    let invoice = billing_repo::find_invoice(&pool, id).await.ok().flatten();

    let invoice = match invoice {
        Some(inv) => inv,
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("請求書が見つかりません"))
                .expect("Response builder should not fail");
        }
    };

    let client_name = order_repo::find_client_name(&pool, invoice.client_id).await
        .unwrap_or_else(|e| { tracing::warn!("invoices: fetch_one failed: {:?}", e); Default::default() });

    let pdf_data = build_client_invoice_pdf_data(&pool, &invoice, &client_name).await;

    let gen = PdfGenerator::new();
    match gen.generate_invoice_pdf(&pdf_data) {
        Ok(pdf_bytes) => {
            let filename = format!("invoice_{}.pdf", invoice.invoice_no);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/pdf")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("inline; filename=\"{}\"", filename),
                )
                .body(Body::from(pdf_bytes))
                .expect("Response builder should not fail")
        }
        Err(e) => {
            tracing::error!("PDF生成エラー: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!("PDF生成エラー: {}", e)))
                .expect("Response builder should not fail")
        }
    }
}
