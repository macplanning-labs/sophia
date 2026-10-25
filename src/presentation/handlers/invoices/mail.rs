/// invoices/mail.rs — 請求書メール送信・プレビュー

use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use crate::domain::models::billing::BillingInvoice;
use crate::infrastructure::repositories::billing_repo;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::api_response::AppError;

/// 対象年月の表示用文字列（例: "2026年06月"）
fn invoice_month_display(invoice: &BillingInvoice) -> String {
    use chrono::Datelike;
    format!("{}年{:02}月", invoice.target_month.year(), invoice.target_month.month())
}

/// 支払期日の表示用文字列（未設定なら空文字）
fn invoice_due_date_display(invoice: &BillingInvoice) -> String {
    invoice.due_date.map(|d| d.format("%Y年%m月%d日").to_string()).unwrap_or_default()
}

/// トークンURL（クライアント確認・PDFダウンロードページ）を組み立てる
fn build_invoice_token_url(invoice: &BillingInvoice) -> String {
    let base_url = std::env::var("BASE_URL").unwrap_or_default();
    format!("{}/token/{}", base_url.trim_end_matches('/'), invoice.uuid)
}

/// このクライアントへの送信が初回か（過去送信実績なし）に応じてテンプレートコードを選ぶ
async fn resolve_invoice_template_code(pool: &PgPool, invoice: &BillingInvoice) -> &'static str {
    match billing_repo::has_prior_sent_invoice(pool, invoice.client_id, invoice.id).await {
        Ok(true) => "client_invoice_send",
        Ok(false) => "client_invoice_send_first",
        Err(e) => {
            tracing::warn!("送信実績チェックエラー（初回テンプレートにフォールバック）: {:?}", e);
            "client_invoice_send_first"
        }
    }
}

/// GET /api/invoices/{id}/email-preview — 送信メール本文プレビュー
///
/// 承認後、フロントの送信モーダルで編集可能な件名・本文の初期値として使う。
/// 宛先（クライアント請求先メール）も返す。
pub async fn api_email_preview(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::{EmailService, compose_invoice_email};

    let invoice = billing_repo::find_invoice(&pool, id).await.log_err().ok().flatten();

    let Some(invoice) = invoice else {
        return Ok((axum::http::StatusCode::NOT_FOUND, "請求書が見つかりません").into_response());
    };

    let client_name = order_repo::find_client_name(&pool, invoice.client_id).await.log_err().unwrap_or_default();
    let (to_email, cc_email) = billing_repo::resolve_invoice_send_recipients(
        &pool, invoice.client_id, invoice.received_order_id,
    ).await.unwrap_or((None, None));

    let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(&pool).await;
    let ctx = compose_invoice_email(
        &client_name,
        &invoice.invoice_no,
        &invoice_month_display(&invoice),
        invoice.total as i64,
        &invoice_due_date_display(&invoice),
        &build_invoice_token_url(&invoice),
        expiry_days,
    );

    let template_code = resolve_invoice_template_code(&pool, &invoice).await;
    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = email_svc.render_template(template_code, &ctx).await?;
    Ok(axum::Json(serde_json::json!({
        "subject": subject,
        "body": body,
        "to_email": to_email,
        "cc_email": cc_email,
        "client_name": client_name,
    })).into_response())
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct SendMailBody {
    /// 送信モーダルで編集された件名・本文。未指定ならテンプレートから生成する。
    pub subject: Option<String>,
    pub body: Option<String>,
}

/// POST /api/invoices/{id}/send — 請求書メール送信（Admin限定、APPROVED→SENT）
pub async fn send_mail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(payload): axum::Json<SendMailBody>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::{EmailService, compose_invoice_email};

    // 請求書情報を取得（承認済み、または送信済み＝再送信のもののみ送信可）
    let invoice = billing_repo::find_invoice(&pool, id).await.log_err().ok().flatten();

    let invoice = match invoice {
        Some(inv) => inv,
        None => return Ok((axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "success": false, "error": "請求書が見つかりません"
        }))).into_response()),
    };

    if !matches!(invoice.status.as_str(), "APPROVED" | "SENT") {
        return Ok((axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
            "success": false, "error": "承認済みまたは送信済みの請求書のみ送信できます"
        }))).into_response());
    }

    if let Some(r) = super::approval::current_guard(&pool, id).await {
        return Ok(r);
    }

    if let Some(msg) = crate::infrastructure::integrations::check::mail_setup_problem(&pool).await {
        return Ok((axum::http::StatusCode::UNPROCESSABLE_ENTITY, axum::Json(serde_json::json!({
            "success": false, "error": msg
        }))).into_response());
    }

    // クライアント請求先メール（受注書 → クライアントマスタの順で解決）
    let (to_email, cc_email) = match billing_repo::resolve_invoice_send_recipients(
        &pool, invoice.client_id, invoice.received_order_id,
    ).await {
        Ok((Some(to), cc)) => (to, cc),
        Ok((None, _)) => {
            tracing::warn!("送信先メールアドレスが設定されていません: invoice={}", invoice.invoice_no);
            return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false,
                "error": "送信先メールアドレスが設定されていません（受注書の請求書送付先、またはクライアントマスタの請求書送付先を設定してください）"
            }))).into_response());
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    // クライアント名
    let client_name = order_repo::find_client_name(&pool, invoice.client_id).await
        .unwrap_or_else(|e| { tracing::warn!("invoices: fetch_one failed: {:?}", e); Default::default() });

    let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(&pool).await;
    let token_url = build_invoice_token_url(&invoice);
    let template_code = resolve_invoice_template_code(&pool, &invoice).await;

    // 件名・本文: 送信モーダルで編集済みならそれを使い、無指定ならテンプレートから生成
    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = match (payload.subject, payload.body) {
        (Some(s), Some(b)) if !s.is_empty() && !b.is_empty() => (s, b),
        _ => {
            let ctx = compose_invoice_email(
                &client_name,
                &invoice.invoice_no,
                &invoice_month_display(&invoice),
                invoice.total as i64,
                &invoice_due_date_display(&invoice),
                &token_url,
                expiry_days,
            );
            email_svc.render_template(template_code, &ctx).await?
        }
    };

    // メール送信（トークンURL経由でSophia上の確認・PDFダウンロード画面へ誘導。PDF添付なし）
    email_svc.send(
        &to_email,
        cc_email.as_deref(),
        &subject,
        &body,
    ).await?;

    // 承諾済みの請求書へ、リンクを送り直したとき(クライアントが失くしたときの救済。DEMO-000223)は、
    // 書類の中身・送信の記録・状態を変えない(承諾した書類は、そのまま残す。リンクは承諾後は期限なし)。
    if invoice.client_accepted_at.is_some() {
        tracing::info!("承諾済みの請求書のリンクを送り直しました: invoice={}", invoice.invoice_no);
        return Ok(axum::Json(serde_json::json!({
            "success": true,
            "message": format!("{}（{}）宛に、承諾済みの請求書のダウンロード用URLをメールで送り直しました(書類の内容は変わりません)", client_name, to_email),
            "to_email": to_email,
            "token_url": token_url,
            "resend_only": true,
        })).into_response());
    }

    // 請求書ステータスをSENTに、受注ステータスをINVOICED→INVOICE_SENTに更新
    if let Err(e) = billing_repo::mark_invoice_sent(&pool, id, &subject, &body).await {
        tracing::error!("DB error: {:?}", e);
    }

    if let Some(ro_id) = invoice.received_order_id {
        if let Err(e) = order_repo::mark_received_order_invoice_sent(&pool, ro_id).await {
            tracing::error!("DB error: {:?}", e);
        }
    }

    // 送った瞬間の PDF を残す(DEMO-000214。失敗しても送信は成功のまま)
    super::pdf::record_invoice_sent(&pool, &invoice, &client_name).await;

    Ok(axum::Json(serde_json::json!({
        "success": true,
        "message": format!("{}（{}）宛に請求書ダウンロード用URLをメール送信しました", client_name, to_email),
        "to_email": to_email,
        "token_url": token_url,
    })).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Utc};

    fn dummy_invoice(target_month: NaiveDate, due_date: Option<NaiveDate>) -> BillingInvoice {
        BillingInvoice {
            id: 1,
            uuid: uuid::Uuid::nil(),
            invoice_no: "SI20260601000001".into(),
            client_id: 1,
            received_order_id: None,
            target_month,
            work_start: target_month,
            work_end: target_month,
            issue_date: target_month,
            due_date,
            subtotal: 0,
            tax_amount: 0,
            total: 0,
            department: String::new(),
            subject: String::new(),
            registration_no: String::new(),
            edi_id: None,
            edi_invoice_no: String::new(),
            source: "SELF".into(),
            status: "DRAFT".into(),
            client_accepted_at: None,
            document_hash: String::new(),
            invoice_pdf: String::new(),
            drive_file_id: String::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            approved_by_id: None,
            approved_at: None,
            sent_at: None,
            sent_subject: String::new(),
            sent_body: String::new(),
            revision: 1,
            supersedes_invoice_id: None,
            revision_reason: None,
            cancelled_at: None,
            cancelled_by_id: None,
            cancel_reason: None,
        }
    }

    #[test]
    fn invoice_month_display_formats_as_japanese_year_month() {
        let invoice = dummy_invoice(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), None);
        assert_eq!(invoice_month_display(&invoice), "2026年06月");
    }

    #[test]
    fn invoice_due_date_display_formats_when_present() {
        let due = NaiveDate::from_ymd_opt(2026, 7, 15).unwrap();
        let invoice = dummy_invoice(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), Some(due));
        assert_eq!(invoice_due_date_display(&invoice), "2026年07月15日");
    }

    #[test]
    fn invoice_due_date_display_empty_when_absent() {
        let invoice = dummy_invoice(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(), None);
        assert_eq!(invoice_due_date_display(&invoice), "");
    }
}
