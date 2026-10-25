/// invoices/pdf.rs — 請求書PDFダウンロード

use crate::infrastructure::db_tx::LogErr;
use axum::extract::{Path, State};
use sqlx::PgPool;

use crate::domain::models::billing::BillingInvoice;
use crate::domain::services::pdf_generator::{InvoicePdfData, InvoicePdfItem};
use crate::infrastructure::repositories::billing_repo;
use crate::infrastructure::repositories::order_repo;

use super::get_company_info;

/// クライアント向け請求書PDFデータを構築する（/pdf・メール添付・Drive再生成で共用）
/// 自社の振込先を読めなければ Err(空の口座番号で請求書を作らない)
pub async fn build_client_invoice_pdf_data(
    pool: &PgPool,
    invoice: &BillingInvoice,
    client_name: &str,
) -> anyhow::Result<InvoicePdfData> {
    let items = billing_repo::list_invoice_items(pool, invoice.id).await
        .unwrap_or_else(|e| { tracing::warn!("invoices: fetch_all failed: {:?}", e); vec![] });

    let company = get_company_info(pool).await?;

    Ok(InvoicePdfData {
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
    })
}

/// GET /invoices/{id}/pdf — 請求書PDFダウンロード
pub async fn download_pdf(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> impl axum::response::IntoResponse {
    // 入口の権限表に頼らず、ハンドラ自身で担当範囲を判定する(詳細と同じ権限・スタンプ。DEMO-000254)
    let stamp = match crate::infrastructure::repositories::stamp_repo::of_invoice(&pool, id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("担当スタンプの取得に失敗: {e:#}");
            return crate::presentation::http_util::forbidden();
        }
    };
    if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::BillingInvoiceView, &stamp) {
        return crate::presentation::http_util::forbidden();
    }
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::presentation::http_util;

    let invoice = billing_repo::find_invoice(&pool, id).await.log_err().ok().flatten();

    let invoice = match invoice {
        Some(inv) => inv,
        None => {
            return http_util::build_response(
                Response::builder().status(StatusCode::NOT_FOUND),
                Body::from("請求書が見つかりません")
            );
        }
    };

    let client_name = order_repo::find_client_name(&pool, invoice.client_id).await
        .unwrap_or_else(|e| { tracing::warn!("invoices: fetch_one failed: {:?}", e); Default::default() });

    match client_invoice_pdf_bytes(&pool, &invoice, &client_name).await {
        Ok(pdf_bytes) => {
            let filename = format!("invoice_{}.pdf", invoice.invoice_no);
            http_util::build_response(
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "application/pdf")
                    .header(
                        header::CONTENT_DISPOSITION,
                        format!("inline; filename=\"{}\"", filename),
                    ),
                Body::from(pdf_bytes)
            )
        }
        Err(e) => {
            tracing::error!("PDF生成エラー: {}", e);
            http_util::build_response(
                Response::builder().status(StatusCode::INTERNAL_SERVER_ERROR),
                Body::from("PDF生成エラー（詳細はサーバのログを確認してください）")
            )
        }
    }
}

/// 請求書の PDF を作る(送るとき・相手が開くとき共通)
pub async fn client_invoice_pdf_bytes(pool: &PgPool, invoice: &BillingInvoice, client_name: &str) -> Result<Vec<u8>, String> {
    let data = build_client_invoice_pdf_data(pool, invoice, client_name).await
        .map_err(|e| format!("請求書データ構築エラー: {e:?}"))?;
    crate::domain::services::pdf_generator::PdfGenerator::new()
        .generate_invoice_pdf(&data)
        .map_err(|e| format!("請求書PDF生成エラー: {e}"))
}

/// 請求書を送った: 送った瞬間の PDF を残す(DEMO-000214)。失敗しても送信は成功のまま
pub async fn record_invoice_sent(pool: &PgPool, invoice: &BillingInvoice, client_name: &str) {
    use crate::domain::services::issued_document::{record_sent, DocType, Issued};
    match client_invoice_pdf_bytes(pool, invoice, client_name).await {
        Ok(pdf) => {
            record_sent(pool, Issued {
                doc_type: DocType::Invoice,
                doc_ref: &invoice.invoice_no,
                party_name: client_name,
                target_month: chrono::Datelike::with_day(&invoice.target_month, 1),
                amount: Some(i64::from(invoice.total)),
                pdf: &pdf,
            })
            .await
        }
        Err(e) => {
            // 送信は成功のまま。PDF が作れなかったことは、ログと異常の通知の両方で知らせる(値は出さない)
            tracing::error!("[発行した書類] 請求書 {} の PDF を作れず、残せませんでした: {e}", invoice.invoice_no);
            let incident = crate::domain::services::chat_notifier::Incident::new(
                integration_core::Level::High,
                format!("請求書 {} を送りましたが、送った PDF を作れませんでした", invoice.invoice_no),
                "送った時点の PDF が残りません。相手がリンクを開くと、その時点の内容で作り直した PDF が出ます",
                "請求書の内容と PDF の部品(Python・reportlab)の状態をサーバのログで確認してください",
            );
            crate::domain::services::chat_notifier::post(&incident).await;
        }
    }
}
