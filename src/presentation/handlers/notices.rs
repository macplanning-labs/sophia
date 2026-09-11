/// presentation/handlers/notices.rs — 支払通知書 CRUD + PDF + メール送信
///
/// Phase 3: 支払通知書の管理。
///
/// ## エンドポイント
/// - GET  /notices                    — 一覧（フィルタ: partner）
/// - POST /notices                    — 作成（発注注文書 + 稼働報告から自動生成）
/// - GET  /notices/{id}               — 詳細
/// - GET  /api/notices/{id}/email-preview — 送信メール本文プレビュー
/// - POST /api/notices/{id}/send      — メール送付（確認用トークンURL）
/// - GET  /api/notices/{id}/pdf       — 支払通知書PDF
/// - GET  /api/notices/{id}/invoice-pdf — パートナー代理請求書PDF

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::domain::models::partner_contract::PaymentNotice;
use crate::domain::services::pdf_generator::{PartnerInvoiceIssuer, PdfGenerator};
use crate::infrastructure::repositories::order_repo::{self, NoticeRow, PartnerInvoiceIssuerRow};
use crate::presentation::api_response::AppError;

// ── フィルタ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct NoticeFilter {
    pub partner: Option<String>,
}

// ── ハンドラ ──

/// POST /notices — 作成（発注注文書IDから自動生成）
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<CreateNoticeForm>,
) -> impl IntoResponse {
    use chrono::Datelike;
    use crate::domain::services::settlement_dashboard::payment_due_date_from_terms;

    let order_id = &form.purchase_order_id;

    // 発注注文書を取得
    let order = order_repo::find_purchase_order_for_notice(&pool, order_id).await.ok().flatten();

    let (partner_id, _project_id, target_month) = match order {
        Some(o) => o,
        None => return Redirect::to("/notices"),
    };

    // 明細を取得
    let items = order_repo::list_order_items_ordered(&pool, order_id).await
        .unwrap_or_else(|e| { tracing::warn!("notices: fetch_all failed: {:?}", e); vec![] });

    // 支払通知書番号採番
    let notice_id = generate_notice_id(&pool, target_month).await;

    // 金額計算（m_tax_rateから対象月時点の実効税率を取得。
    // 税率ごとの端数処理はtax_calculation::calculate_tax_breakdownに一元化）
    let subtotal: i32 = items.iter().map(|i| i.amount).sum();
    let tax_rate = crate::infrastructure::repositories::tax_rate_repo::find_effective_rate(&pool, target_month)
        .await
        .unwrap_or(rust_decimal::Decimal::from(10));
    let breakdown = crate::domain::services::tax_calculation::calculate_tax_breakdown(
        &[(tax_rate, subtotal)],
    );
    let tax_amount = crate::domain::services::tax_calculation::total_tax_amount(&breakdown);
    let total = subtotal + tax_amount;

    // 支払期日: 発注/契約の支払条件から算出（請求書 due_date と同じロジック）
    let month_end = if target_month.month() == 12 {
        chrono::NaiveDate::from_ymd_opt(target_month.year() + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(target_month.year(), target_month.month() + 1, 1)
    }
    .and_then(|d| d.pred_opt())
    .unwrap_or(target_month);
    let payment_condition = order_repo::find_purchase_order_payment_condition(&pool, order_id)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let payment_due_date = Some(payment_due_date_from_terms(month_end, &payment_condition));

    // 支払通知書作成
    if let Err(e) = order_repo::insert_payment_notice(
        &pool, &notice_id, order_id, &partner_id, target_month, payment_due_date, subtotal, tax_amount, total,
    ).await {
        tracing::error!("DB error: {:?}", e);
    }

    // 明細をコピー
    for item in &items {
        if let Err(e) = order_repo::insert_payment_notice_item(
            &pool, &notice_id, item.partner_contract_id, item.actual_hours, item.base_fee, item.effort,
            item.lower_limit_hours, item.upper_limit_hours, item.fixed_hours, item.deduction_rate, item.overtime_rate, item.amount,
            tax_rate,
        ).await {
            tracing::error!("DB error: {:?}", e);
        }
    }

    // 発注注文書のステータスを NOTICE_CREATED に更新
    if let Err(e) = order_repo::mark_purchase_order_notice_created(&pool, order_id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/notices/{}", notice_id))
}

/// 支払通知書メールの件名・本文組み立てに必要な共通情報
async fn build_notice_email_ctx(
    pool: &PgPool,
    id: &str,
) -> Result<(PaymentNotice, String, String, String, std::collections::HashMap<String, String>, String), (axum::http::StatusCode, String)> {
    let notice = order_repo::find_payment_notice(pool, id).await.ok().flatten()
        .ok_or((axum::http::StatusCode::NOT_FOUND, "支払通知書が見つかりません".to_string()))?;

    let partner_info = order_repo::find_partner_name_email_cc(pool, &notice.partner_id).await.ok().flatten();
    let (partner_name, partner_email, partner_cc) = partner_info.unwrap_or_default();

    if partner_email.is_empty() {
        return Err((axum::http::StatusCode::BAD_REQUEST, "パートナーのメールアドレスが設定されていません".to_string()));
    }

    let target_month = notice.target_month.format("%Y年%m月").to_string();
    let base_url = std::env::var("BASE_URL").unwrap_or_default();
    let token_url = format!("{}/token/{}", base_url.trim_end_matches('/'), notice.uuid);
    let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(pool).await;
    let ctx = crate::domain::services::email_service::compose_payment_notice_email(
        &partner_name, &notice.notice_id, &target_month, &token_url, expiry_days,
    );

    Ok((notice, partner_name, partner_email, partner_cc, ctx, token_url))
}

/// GET /api/notices/{id}/email-preview — 送信メール本文プレビュー
///
/// フロントの送信モーダルで編集可能な件名・本文の初期値として使う。
/// 宛先（パートナーメール）・CC（パートナーマスタのCC欄）も返す。
pub async fn api_email_preview(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::EmailService;

    let (_notice, partner_name, partner_email, partner_cc, ctx, _token_url) =
        match build_notice_email_ctx(&pool, &id).await {
            Ok(v) => v,
            Err((status, msg)) => return Ok((status, axum::Json(serde_json::json!({ "error": msg }))).into_response()),
        };

    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = email_svc.render_template("notice_send", &ctx).await?;
    Ok(axum::Json(serde_json::json!({
        "subject": subject,
        "body": body,
        "to_email": partner_email,
        "cc_email": partner_cc,
        "partner_name": partner_name,
    })).into_response())
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct SendMailBody {
    /// 送信モーダルで編集された件名・本文。未指定ならテンプレートから生成する。
    pub subject: Option<String>,
    pub body: Option<String>,
}

/// POST /api/notices/{id}/send — 支払通知書メール送付（確認用トークンURL。PDF添付なし）
pub async fn send_mail(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    axum::Json(payload): axum::Json<SendMailBody>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::EmailService;

    let (_notice, _partner_name, partner_email, partner_cc, ctx, token_url) =
        match build_notice_email_ctx(&pool, &id).await {
            Ok(v) => v,
            Err((status, msg)) => return Ok((status, axum::Json(serde_json::json!({ "success": false, "error": msg }))).into_response()),
        };

    let email_svc = EmailService::new(pool.clone());

    // 件名・本文: 送信モーダルで編集済みならそれを使い、無指定ならテンプレートから生成
    let (subject, body) = match (payload.subject, payload.body) {
        (Some(s), Some(b)) if !s.is_empty() && !b.is_empty() => (s, b),
        _ => email_svc.render_template("notice_send", &ctx).await?,
    };

    let cc = if partner_cc.trim().is_empty() { None } else { Some(partner_cc.as_str()) };
    email_svc.send(&partner_email, cc, &subject, &body).await?;

    if let Err(e) = order_repo::mark_payment_notice_mail_sent(&pool, &id).await {
        tracing::error!("mail_sent_at更新エラー: {:?}", e);
    }

    Ok(axum::Json(serde_json::json!({
        "success": true,
        "message": "支払通知書・請求書の確認URLをメール送信しました",
        "token_url": token_url,
    })).into_response())
}

fn pdf_response(pdf_bytes: Vec<u8>, filename: &str) -> axum::response::Response {
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use axum::response::Response;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/pdf")
        .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"{}\"", filename))
        .body(Body::from(pdf_bytes))
        .expect("Response builder should not fail")
}

fn pdf_error(status: axum::http::StatusCode, msg: String) -> axum::response::Response {
    use axum::body::Body;
    use axum::response::Response;
    Response::builder()
        .status(status)
        .body(Body::from(msg))
        .expect("Response builder should not fail")
}

/// GET /api/notices/{id}/pdf — 支払通知書PDF
pub async fn download_pdf(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    let notice = match order_repo::find_payment_notice(&pool, &id).await.ok().flatten() {
        Some(n) => n,
        None => return pdf_error(StatusCode::NOT_FOUND, "支払通知書が見つかりません".into()),
    };

    match generate_payment_notice_pdf_bytes(&pool, &notice).await {
        Ok(bytes) => pdf_response(bytes, &format!("notice_{}.pdf", notice.notice_id)),
        Err(e) => {
            tracing::error!("{}", e);
            pdf_error(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    }
}

/// GET /api/notices/{id}/invoice-pdf — パートナー代理請求書PDF
pub async fn download_invoice_pdf(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    let notice = match order_repo::find_payment_notice(&pool, &id).await.ok().flatten() {
        Some(n) => n,
        None => return pdf_error(StatusCode::NOT_FOUND, "支払通知書が見つかりません".into()),
    };

    match generate_partner_invoice_pdf_bytes(&pool, &notice).await {
        Ok(bytes) => pdf_response(bytes, &format!("invoice_{}.pdf", notice.notice_id)),
        Err(e) => {
            tracing::error!("{}", e);
            pdf_error(StatusCode::INTERNAL_SERVER_ERROR, e)
        }
    }
}

// ── フォーム ──

#[derive(Debug, serde::Deserialize)]
pub struct CreateNoticeForm {
    pub purchase_order_id: String,
}

/// 支払通知書番号を自動採番する（PN-YYYYMM-001）
async fn generate_notice_id(pool: &PgPool, target_month: chrono::NaiveDate) -> String {
    let prefix = format!("PN-{}", target_month.format("%Y%m"));
    let max_seq = order_repo::find_max_notice_id_with_prefix(pool, &format!("{}%", prefix)).await
        .ok()
        .flatten();

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    format!("{}-{:03}", prefix, next_seq)
}

/// 支払通知書PDFデータ構築（共通関数）
///
/// notices.rs / token.rs の全PDFハンドラから呼ばれる唯一のデータ構築ロジック。
/// フィールド追加時はここだけ修正すればOK。
pub async fn build_payment_notice_pdf_data(
    pool: &PgPool,
    notice: &PaymentNotice,
) -> anyhow::Result<crate::domain::services::pdf_generator::PaymentNoticePdfData> {
    use crate::domain::services::pdf_generator::{PaymentNoticePdfData, PaymentNoticeItem as PdfNoticeItem};

    let partner_name = order_repo::find_partner_name(pool, &notice.partner_id).await
        .unwrap_or_else(|e| { tracing::warn!("build_pdf: {:?}", e); Default::default() });

    let items = order_repo::list_payment_notice_items(pool, &notice.notice_id).await
        .unwrap_or_else(|e| { tracing::warn!("build_pdf: {:?}", e); vec![] });

    // 技術者名を取得
    let mut engineer_names: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    for item in &items {
        let name = order_repo::find_engineer_name_for_partner_contract(pool, item.partner_contract_id).await.ok().flatten();
        if let Some(n) = name {
            engineer_names.insert(item.partner_contract_id, n);
        }
    }

    let company = order_repo::find_company_bank_info(pool).await.ok().flatten();
    let (company_name, bank_name, bank_branch, account_type, account_number, account_name) = company.unwrap_or_default();

    // 自社住所等（請求書PDFの宛先表示用）
    let company_detail = crate::infrastructure::repositories::billing_repo::find_company_invoice_info(pool)
        .await.ok().flatten();

    let project_name = order_repo::find_project_name_by_purchase_order(pool, &notice.purchase_order_id)
        .await.ok().flatten();

    let target_month = notice.target_month.format("%Y年%m月").to_string();

    Ok(PaymentNoticePdfData {
        notice_id: notice.notice_id.clone(),
        notice_date: notice.notice_date,
        partner_name,
        target_month,
        items: items.iter().map(|i| {
            // adjustment 未保存（0）の既存データでも、精算後金額から超過/控除を復元する
            let adjustment = if i.adjustment != 0 {
                i.adjustment as i64
            } else {
                i.amount as i64 - i.base_fee as i64
            };
            let (excess_amount, shortage_amount) = if adjustment >= 0 {
                (Some(adjustment), Some(0))
            } else {
                (Some(0), Some(-adjustment))
            };
            PdfNoticeItem {
                description: engineer_names.get(&i.partner_contract_id)
                    .cloned().unwrap_or_else(|| format!("契約#{}", i.partner_contract_id)),
                unit_price: i.base_fee as i64,
                quantity: i.effort.to_string(),
                amount: i.amount as i64,
                engineer_name: engineer_names.get(&i.partner_contract_id).cloned(),
                base_fee: Some(i.base_fee as i64),
                excess_amount,
                shortage_amount,
                actual_hours: Some(i.actual_hours.to_string().parse::<f64>().unwrap_or(0.0)),
                deduction_rate: Some(i.deduction_rate as i64),
                overtime_rate: Some(i.overtime_rate as i64),
                lower_limit_hours: Some(i.lower_limit_hours.to_string().parse::<f64>().unwrap_or(0.0)),
                upper_limit_hours: Some(i.upper_limit_hours.to_string().parse::<f64>().unwrap_or(0.0)),
            }
        }).collect(),
        subtotal: notice.subtotal as i64,
        tax_amount: notice.tax_amount as i64,
        total: notice.total as i64,
        payment_date: notice.payment_due_date,
        company_name: company_detail.as_ref().map(|c| c.name.clone()).unwrap_or(company_name),
        bank_name,
        bank_branch,
        account_type,
        account_number,
        account_name,
        project_name,
        company_postal_code: company_detail.as_ref().map(|c| c.postal_code.clone()),
        company_address: company_detail.as_ref().map(|c| c.address.clone()),
        company_tel: company_detail.as_ref().map(|c| c.tel.clone()),
        ..Default::default()
    })
}

fn issuer_from_row(row: PartnerInvoiceIssuerRow) -> PartnerInvoiceIssuer {
    PartnerInvoiceIssuer {
        name: row.name,
        postal_code: row.postal_code,
        address: row.address,
        tel: row.tel,
        representative: row.representative,
        registration_no: row.registration_no,
        bank_name: row.bank_name,
        bank_branch: row.bank_branch,
        account_type: row.account_type,
        account_number: row.account_number,
        account_name: row.account_name,
    }
}

/// 支払通知書PDFバイト列を生成する
pub async fn generate_payment_notice_pdf_bytes(
    pool: &PgPool,
    notice: &PaymentNotice,
) -> Result<Vec<u8>, String> {
    let pdf_data = build_payment_notice_pdf_data(pool, notice).await
        .map_err(|e| format!("PDFデータ構築エラー: {}", e))?;
    PdfGenerator::new().generate_payment_notice_pdf(&pdf_data)
        .map_err(|e| format!("支払通知書PDF生成エラー: {}", e))
}

/// パートナー代理請求書PDFバイト列を生成する
pub async fn generate_partner_invoice_pdf_bytes(
    pool: &PgPool,
    notice: &PaymentNotice,
) -> Result<Vec<u8>, String> {
    let pdf_data = build_payment_notice_pdf_data(pool, notice).await
        .map_err(|e| format!("PDFデータ構築エラー: {}", e))?;
    let issuer_row = order_repo::find_partner_invoice_issuer(pool, &notice.partner_id).await
        .ok().flatten()
        .unwrap_or_else(|| PartnerInvoiceIssuerRow {
            name: pdf_data.partner_name.clone(),
            ..Default::default()
        });
    let issuer = issuer_from_row(issuer_row);
    PdfGenerator::new().generate_partner_invoice_pdf(&pdf_data, &issuer)
        .map_err(|e| format!("請求書PDF生成エラー: {}", e))
}

// ══════════════════════════════════════════════════════════
// SPA用 JSON API
// ══════════════════════════════════════════════════════════

/// GET /api/notices — 一覧（JSON）
pub async fn api_index(
    State(pool): State<PgPool>,
    Query(filter): Query<NoticeFilter>,
) -> axum::Json<Vec<NoticeRow>> {
    let partner = filter.partner.filter(|s| !s.is_empty());

    let notices = order_repo::list_notice_rows(&pool, partner.as_deref()).await
        .unwrap_or_else(|e| { tracing::warn!("notices api: {:?}", e); vec![] });

    axum::Json(notices)
}

/// POST /api/notices/{id}/delete — 削除（JSON）
///
/// パートナーが受諾済み（partner_accepted_at設定済み）の支払通知は、既にパートナー側に
/// 見えている正式な文書のため削除不可とする（発注書=DRAFTのみ・受注書=REGISTEREDのみ・
/// 請求書=PENDING_APPROVALのみ削除可、と同じ「相手方が動く前まで」の方針）。
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let notice = order_repo::find_payment_notice(&pool, &id).await.ok().flatten();

    match notice {
        Some(n) if n.partner_accepted_at.is_none() => {}
        Some(_) => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false, "error": "パートナー未受諾の支払通知のみ削除できます"
            }))).into_response());
        }
        None => {
            return Ok((axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                "success": false, "error": "支払通知が見つかりません"
            }))).into_response());
        }
    }

    order_repo::delete_payment_notice(&pool, &id).await?;
    tracing::info!("支払通知削除: notice_id={}", id);

    Ok(axum::Json(serde_json::json!({ "success": true, "message": "削除しました" })).into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct ApiUpdateNoticeHeaderForm {
    pub notice_date: String,
    pub payment_due_date: Option<String>,
    pub remarks: String,
}

/// PUT /api/v1/notices/{id} — ヘッダ情報の更新（JSON。明細は対象外・通知日/支払期日/備考のみ）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    axum::Json(form): axum::Json<ApiUpdateNoticeHeaderForm>,
) -> Result<impl IntoResponse, AppError> {
    let notice_date = match chrono::NaiveDate::parse_from_str(&form.notice_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false, "error": "通知日の形式が不正です"
            }))).into_response());
        }
    };
    let payment_due_date = form.payment_due_date.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    order_repo::update_payment_notice_header(&pool, &id, notice_date, payment_due_date, &form.remarks).await?;
    tracing::info!("支払通知ヘッダ更新: notice_id={}", id);

    Ok(axum::Json(serde_json::json!({ "success": true })).into_response())
}

/// GET /api/notices/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let notice = order_repo::find_payment_notice(&pool, &id).await.ok().flatten();

    match notice {
        Some(notice) => {
            let items = order_repo::list_payment_notice_items(&pool, &id).await.unwrap_or_default();

            let partner_name = order_repo::find_partner_name(&pool, &notice.partner_id).await.unwrap_or_default();

            let target_month_display = notice.target_month.format("%Y年%m月").to_string();

            // 金額計算（明細のtax_rateごとにグルーピングして端数処理）
            let subtotal: i64 = items.iter().map(|i| i.amount as i64).sum();
            let breakdown = crate::domain::services::tax_calculation::calculate_tax_breakdown(
                &items.iter().map(|i| (i.tax_rate, i.amount)).collect::<Vec<_>>(),
            );
            let tax_amount = crate::domain::services::tax_calculation::total_tax_amount(&breakdown) as i64;
            let total = subtotal + tax_amount;

            axum::Json(serde_json::json!({
                "notice": notice,
                "items": items,
                "partner_name": partner_name,
                "target_month_display": target_month_display,
                "purchase_order_id": notice.purchase_order_id,
                "subtotal": subtotal,
                "tax_amount": tax_amount,
                "total": total,
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
