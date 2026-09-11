/// home/edi.rs — EDI-OASIS連携（注文書/請求書取込・承諾・PDF生成）

use axum::{
    extract::{State, Path, Query, Json},
    response::IntoResponse,
    http::StatusCode,
};
use sqlx::PgPool;
use chrono::Datelike;

use super::StatusResponse;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EDI-OASIS 注文書取込
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// POST /api/edi/import — 未処理EDIメール一括取込
///
/// mail_pipeline::run_pipeline（Phase1〜4）を実行する。Phase2がEDI-OASISの当月+翌月を
/// ポーリングするため、個々のメールの有無に関わらず未取込の注文/請求を拾える。
pub async fn edi_import(
    State(pool): State<PgPool>,
) -> (StatusCode, Json<StatusResponse>) {
    let result = crate::infrastructure::mail_pipeline::run_pipeline(&pool, None).await;
    let has_errors = result.has_errors();
    let msg = result.to_string();
    (StatusCode::OK, Json(StatusResponse {
        ok: !has_errors,
        error: Some(msg),
    }))
}

/// POST /api/edi/import/{mail_id} — 指定メールからEDI取込
///
/// ダッシュボードの「受注」ボタンから呼ばれる。
/// mail_id は t_mail_scan_log の ID。
/// メール本文から EDI URL を抽出 → OASIS API → 受注登録。
pub async fn edi_import_single(
    State(pool): State<PgPool>,
    Path(mail_id): Path<i64>,
) -> (StatusCode, Json<StatusResponse>) {
    use crate::infrastructure::edi_oasis_client::EdiOasisClient;
    use crate::infrastructure::edi_order_importer::import_from_mail_scan_log;

    // 1. t_mail_scan_log からメール本文を取得
    let body: Option<String> = crate::infrastructure::repositories::mail_repo::find_mail_body(&pool, mail_id)
        .await
        .ok()
        .flatten();

    let body = match body {
        Some(b) if !b.is_empty() => b,
        _ => {
            return (StatusCode::NOT_FOUND, Json(StatusResponse {
                ok: false,
                error: Some("メールが見つからないか、本文がありません".into()),
            }));
        }
    };

    // 2. URL 抽出
    let detail_id = match EdiOasisClient::extract_order_detail_id(&body) {
        Some(id) => id,
        None => {
            return (StatusCode::BAD_REQUEST, Json(StatusResponse {
                ok: false,
                error: Some("メール本文からEDI-OASIS URLを検出できませんでした".into()),
            }));
        }
    };

    // 3. EDI取込実行
    match import_from_mail_scan_log(&pool, mail_id, detail_id).await {
        Ok(result) => {
            (StatusCode::OK, Json(StatusResponse {
                ok: true,
                error: Some(format!(
                    "取込{}件, スキップ{}件",
                    result.imported, result.skipped
                )),
            }))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
                ok: false,
                error: Some(e),
            }))
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OASIS 注文一覧・直接取込
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// GET /api/edi/orders?year=2026&month=6 — OASIS注文一覧を取得
pub async fn edi_list_orders(
    State(pool): State<PgPool>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    use crate::infrastructure::edi_oasis_client::EdiOasisClient;

    let now = chrono::Local::now();
    let year: i32 = params.get("year")
        .and_then(|v| v.parse().ok())
        .unwrap_or(now.year());
    let month: i32 = params.get("month")
        .and_then(|v| v.parse().ok())
        .unwrap_or(now.month() as i32);

    let mut client = match EdiOasisClient::from_env() {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": format!("{e}")
        }))),
    };

    if let Err(e) = client.login().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": format!("ログイン失敗: {e}")
        })));
    }

    match client.fetch_orders(year, month).await {
        Ok(orders) => {
            // Sophia側の取込済判定: t_received_order.client_order_number でチェック
            let target_month = format!("{:04}-{:02}", year, month);
            let imported_orders: Vec<String> = crate::infrastructure::repositories::order_repo::find_imported_client_order_numbers(&pool, &target_month)
                .await
                .unwrap_or_default();

            let mut enriched: Vec<serde_json::Value> = Vec::new();
            for o in &orders {
                let mut v = serde_json::to_value(o).unwrap_or_default();
                if let Some(order_no) = v.get("order_no").and_then(|n| n.as_str()) {
                    let is_imported = imported_orders.iter().any(|imp| imp == order_no);
                    v.as_object_mut().unwrap().insert("is_imported".to_string(), serde_json::json!(is_imported));
                }
                enriched.push(v);
            }

            (StatusCode::OK, Json(serde_json::json!({
                "ok": true,
                "year": year,
                "month": month,
                "orders": enriched,
            })))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "ok": false, "error": format!("{e}")
            })))
        }
    }
}

/// POST /api/edi/orders/{order_id}/import — OASIS注文を取込+受領
pub async fn edi_import_and_approve(
    State(pool): State<PgPool>,
    Path(order_id): Path<i64>,
) -> (StatusCode, Json<StatusResponse>) {
    use crate::infrastructure::edi_order_importer::import_from_oasis_direct;

    match import_from_oasis_direct(&pool, order_id).await {
        Ok(result) => {
            (StatusCode::OK, Json(StatusResponse {
                ok: true,
                error: Some(format!(
                    "取込{}件, スキップ{}件",
                    result.imported, result.skipped
                )),
            }))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
                ok: false,
                error: Some(e),
            }))
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OASIS 請求書一覧・承諾
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// GET /api/edi/invoices?year=2026&month=6 — OASIS請求書一覧を取得
pub async fn edi_list_invoices(
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    use crate::infrastructure::edi_oasis_client::EdiOasisClient;

    let now = chrono::Local::now();
    let year: i32 = params.get("year")
        .and_then(|v| v.parse().ok())
        .unwrap_or(now.year());
    let month: i32 = params.get("month")
        .and_then(|v| v.parse().ok())
        .unwrap_or(now.month() as i32);

    let mut client = match EdiOasisClient::from_env() {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": format!("{e}")
        }))),
    };

    if let Err(e) = client.login().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "ok": false, "error": format!("ログイン失敗: {e}")
        })));
    }

    match client.fetch_invoices(year, month).await {
        Ok(invoices) => {
            (StatusCode::OK, Json(serde_json::json!({
                "ok": true,
                "year": year,
                "month": month,
                "invoices": invoices,
            })))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "ok": false, "error": format!("{e}")
            })))
        }
    }
}

/// POST /api/edi/invoices/{invoice_id}/approve — OASIS請求書を受領（HTML保存+承諾）
pub async fn edi_approve_invoice(
    Query(params): Query<std::collections::HashMap<String, String>>,
    Path(invoice_id): Path<i64>,
) -> (StatusCode, Json<StatusResponse>) {
    use crate::infrastructure::edi_oasis_client::EdiOasisClient;
    use crate::infrastructure::drive_service;

    let mut client = match EdiOasisClient::from_env() {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
            ok: false,
            error: Some(format!("{e}")),
        })),
    };

    if let Err(e) = client.login().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
            ok: false,
            error: Some(format!("ログイン失敗: {e}")),
        }));
    }

    // 請求番号はフロントから渡す（ファイル名用）
    let invoice_no = params.get("invoice_no").cloned().unwrap_or_default();
    let target_month = params.get("month").cloned().unwrap_or_default();

    // 1. 請求書HTMLをダウンロード → Google Drive保存
    match client.download_invoice_html(invoice_id).await {
        Ok(html_bytes) => {
            let filename = if !invoice_no.is_empty() {
                format!("請求書_株式会社イー・ビジネス_{}.html", invoice_no)
            } else if !target_month.is_empty() {
                format!("請求書_株式会社イー・ビジネス_{}.html", target_month)
            } else {
                format!("請求書_株式会社イー・ビジネス_{}.html", invoice_id)
            };
            let (file_id, link) = drive_service::upload_document(
                "client", "株式会社イー・ビジネス", "invoice",
                &filename, &html_bytes, Some("text/html"),
            ).await.unwrap_or_default();
            if !file_id.is_empty() {
                tracing::info!("[EDI請求書] Google Drive保存: {} → {}", filename, link);
            }
        }
        Err(e) => tracing::warn!("[EDI請求書] HTMLダウンロード失敗: {}", e),
    }

    // 2. OASIS承諾
    match client.approve_invoice(invoice_id).await {
        Ok(()) => {
            (StatusCode::OK, Json(StatusResponse {
                ok: true,
                error: Some("受領完了（Drive保存+承諾）".to_string()),
            }))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
                ok: false,
                error: Some(format!("{e}")),
            }))
        }
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// EDI 統合一括取込（注文書 + 請求書 + PDF + Google Drive）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// POST /api/edi/import-all — 注文書+請求書の統合一括取込
///
/// 1. 未処理EDIメールから注文書を取込
/// 2. 指定年月の請求書をEDI-OASISから取込
/// 3. バックグラウンドでPDF生成 + Google Driveアップロード
pub async fn edi_import_all(
    State(pool): State<PgPool>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> (StatusCode, Json<serde_json::Value>) {
    use crate::infrastructure::billing_importer::import_billings_for_month;
    use crate::infrastructure::edi_oasis_client::EdiOasisClient;

    let year = payload["year"].as_i64().unwrap_or(0) as i32;
    let month = payload["month"].as_i64().unwrap_or(0) as i32;

    let mut messages: Vec<String> = Vec::new();
    let mut has_error = false;

    // ── 1. 注文書取込（mail_pipeline::run_pipeline — Phase1〜4） ──
    // 年月が指定されていればPhase2のEDI-OASISポーリングをその月に限定する。
    // 未指定(0)の場合は従来通り当月+翌月を自動ポーリングする。
    let override_month = if year > 0 && month > 0 { Some((year, month)) } else { None };
    let pipeline_result = crate::infrastructure::mail_pipeline::run_pipeline(&pool, override_month).await;
    if pipeline_result.has_errors() {
        has_error = true;
        messages.push(format!("注文書取込エラー: {pipeline_result}"));
    } else {
        messages.push(format!("注文書取込: {pipeline_result}"));
    }
    // 新規に何が取り込まれたか（0件なら「新規なし」を明示する）
    let new_orders = &pipeline_result.phase3.new_orders;
    if new_orders.is_empty() {
        messages.push("新しい注文書はありません（対象は全て取込済みです）".to_string());
    } else {
        messages.push(format!(
            "新規注文書 {}件: {}",
            new_orders.len(),
            new_orders.join(" / ")
        ));
    }
    let new_invoice_links = &pipeline_result.phase3.new_invoices;
    if !new_invoice_links.is_empty() {
        messages.push(format!(
            "新規に突合できた支払通知書/請求書 {}件: {}",
            new_invoice_links.len(),
            new_invoice_links.join(" / ")
        ));
    }

    // ── 2. 請求書取込 ──
    if year > 0 && month > 0 {
        let client = match EdiOasisClient::from_env() {
            Ok(c) => c,
            Err(e) => {
                has_error = true;
                messages.push(format!("EDI接続エラー: {}", e));
                let combined = messages.join(" / ");
                return (StatusCode::OK, Json(serde_json::json!({
                    "ok": !has_error,
                    "error": combined,
                })));
            }
        };
        let mut client = client;
        let billing_result = import_billings_for_month(&pool, &mut client, year, month).await;
        if billing_result.errors.is_empty() {
            messages.push(format!(
                "請求書: 明細{}件, スキップ{}件",
                billing_result.billings_imported, billing_result.billings_skipped
            ));
            if billing_result.new_billings.is_empty() {
                messages.push("新しい請求明細はありません（対象は全て取込済みです）".to_string());
            } else {
                messages.push(format!(
                    "新規請求明細 {}件: {}",
                    billing_result.new_billings.len(),
                    billing_result.new_billings.join(" / ")
                ));
            }
        } else {
            has_error = true;
            messages.push(format!("請求書エラー: {}", billing_result.errors.join("; ")));
        }
    } else {
        messages.push("請求書: 年月未指定のためスキップ".to_string());
    }

    // ── 3. バックグラウンドでPDF生成 + Google Driveアップロード ──
    let bg_pool = pool.clone();
    tokio::spawn(async move {
        if let Err(e) = generate_and_upload_pdfs(&bg_pool).await {
            tracing::error!("[一括取込] PDF生成/Driveアップロードエラー: {}", e);
        }
    });

    let combined = messages.join(" / ");
    (StatusCode::OK, Json(serde_json::json!({
        "ok": !has_error,
        "error": combined,
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
        .unwrap_or_default();

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
            .unwrap_or_default();

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
        .unwrap_or_default();

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
            .unwrap_or_default();

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
