/// presentation/handlers/token.rs — パートナートークンアクセス
///
/// 認証不要。トークンUUIDで注文書/支払通知書を特定し、表示・承諾処理を行う。
/// サーバー側で直接HTMLをレンダリングする（invite.rs / auth/portal.rsと同じ方式。
/// Next.js SPA側には対応ページを持たない）。
/// PDFデータ構築は orders.rs / notices.rs の共通関数に委譲（重複防止）。
///
/// ## エンドポイント
/// - GET  /token/{uuid}                — 注文書/支払通知書確認ページ
/// - POST /token/{uuid}/accept         — 承諾処理
/// - GET  /token/{uuid}/pdf            — PDF（注文書 or 支払通知書）
/// - GET  /token/{uuid}/invoice-pdf    — パートナー代理請求書PDF（支払通知のみ）
/// - GET  /token/{uuid}/acceptance-pdf  — 注文請書PDFダウンロード
/// - POST /token/{uuid}/timesheet      — 稼働報告アップロード（受諾済み注文書のみ）

use axum::{
    extract::{Multipart, Path, Query, State},
    response::{IntoResponse, Redirect},
    Json,
};
use sqlx::PgPool;
use std::collections::HashMap;

use crate::domain::models::partner_contract::{PaymentNotice, PurchaseOrder};
use crate::infrastructure::repositories::order_repo;
use crate::presentation::handlers::timesheet_upload_common;

/// トークンの有効期限チェック（DB設定値から取得）
async fn is_token_expired(pool: &PgPool, issued_at: Option<chrono::DateTime<chrono::Utc>>) -> bool {
    let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(pool).await;
    match issued_at {
        Some(issued) => {
            let now = chrono::Utc::now();
            now.signed_duration_since(issued).num_days() > expiry_days as i64
        }
        None => false,
    }
}

/// 共通ページ枠（invite.rs / auth/portal.rs と同じダークテーマのカードスタイル）
fn page_shell(title: &str, body: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<style>
  body {{ font-family: -apple-system, sans-serif; background: #0a0a0a; color: #e5e5e5; display: flex; justify-content: center; padding: 40px 16px; margin: 0; }}
  .card {{ background: #1a1a1a; border: 1px solid #333; border-radius: 12px; padding: 32px; max-width: 640px; width: 100%; }}
  h1 {{ font-size: 24px; background: linear-gradient(to right, #34d399, #2dd4bf); -webkit-background-clip: text; -webkit-text-fill-color: transparent; margin: 0 0 4px; }}
  h2 {{ font-size: 15px; color: #d4d4d4; margin: 24px 0 8px; border-bottom: 1px solid #333; padding-bottom: 6px; }}
  p {{ color: #a3a3a3; font-size: 14px; line-height: 1.6; }}
  table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
  th, td {{ text-align: left; padding: 6px 8px; border-bottom: 1px solid #262626; }}
  th {{ color: #737373; font-weight: 500; width: 30%; }}
  .items th {{ width: auto; color: #737373; }}
  .items td.num, .items th.num {{ text-align: right; }}
  .banner {{ padding: 12px 16px; border-radius: 8px; font-size: 14px; margin-bottom: 16px; }}
  .banner.ok {{ background: #052e16; border: 1px solid #14532d; color: #86efac; }}
  .banner.warn {{ background: #451a03; border: 1px solid #78350f; color: #fdba74; }}
  .actions {{ margin-top: 24px; display: flex; gap: 12px; flex-wrap: wrap; }}
  button, .btn {{ padding: 12px 20px; border: none; border-radius: 8px; font-size: 14px; font-weight: 600; cursor: pointer; text-decoration: none; display: inline-block; }}
  button {{ background: linear-gradient(to right, #10b981, #14b8a6); color: white; }}
  button:hover {{ opacity: 0.9; }}
  .btn.secondary {{ background: #262626; color: #e5e5e5; border: 1px solid #404040; }}
</style>
</head><body>
<div class="card">
  <h1>Sophia</h1>
  {body}
</div>
</body></html>"#
    )
}

fn error_page(message: &str) -> String {
    page_shell("Sophia", &format!("<p>{}</p>", message))
}

/// 注文書/支払通知書表示（GET /token/:uuid）
/// 認証不要。トークンURLでアクセスされるパートナー向け確認ページ（サーバー側HTMLレンダリング）。
pub async fn view(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    use axum::response::Html;

    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => return Html(error_page("無効なリンクです。")).into_response(),
    };

    // アップロード結果バナー（POST /token/:uuid/timesheet からのリダイレクト経由）
    let upload_message: Option<(bool, String)> = if let Some(msg) = query.get("uploaded") {
        Some((true, msg.clone()))
    } else {
        query.get("upload_error").map(|msg| (false, msg.clone()))
    };

    // ── 注文書を検索 ──
    match order_repo::find_purchase_order_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(order)) => {
            if order.status == "SENT" && is_token_expired(&pool, order.token_issued_at).await {
                return Html(error_page(
                    "このリンクの有効期限が切れています。お手数ですが発注元までお問い合わせください。"
                )).into_response();
            }
            return Html(render_order_page(&pool, &order, &uuid, upload_message).await).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンリンク解決] 処理=注文書検索 影響=確認ページを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Html(error_page(
                "お手数ですが、しばらく時間をおいてからアクセスしてください。"
            ))).into_response();
        }
    }

    // ── 支払通知書を検索 ──
    match order_repo::find_payment_notice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(notice)) => {
            return Html(render_notice_page(&pool, &notice, &uuid).await).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンリンク解決] 処理=支払通知書検索 影響=確認ページを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Html(error_page(
                "お手数ですが、しばらく時間をおいてからアクセスしてください。"
            ))).into_response();
        }
    }

    // ── クライアント向け請求書を検索 ──
    use crate::infrastructure::repositories::billing_repo;
    match billing_repo::find_invoice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(invoice)) => {
            if is_token_expired(&pool, invoice.sent_at).await {
                return Html(error_page(
                    "このリンクの有効期限が切れています。お手数ですが発注元までお問い合わせください。"
                )).into_response();
            }
            return Html(render_invoice_page(&pool, &invoice, &uuid).await).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンリンク解決] 処理=請求書検索 影響=確認ページを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Html(error_page(
                "お手数ですが、しばらく時間をおいてからアクセスしてください。"
            ))).into_response();
        }
    }

    (axum::http::StatusCode::NOT_FOUND, Html(error_page("リンクが見つかりません。"))).into_response()
}

/// クライアント向け請求書の確認・ダウンロードページ
async fn render_invoice_page(pool: &PgPool, invoice: &crate::domain::models::billing::BillingInvoice, uuid: &str) -> String {
    use chrono::Datelike;

    let client_name = order_repo::find_client_name(pool, invoice.client_id).await.unwrap_or_default();
    let company_name = order_repo::find_company_name_setting(pool).await.unwrap_or_else(|_| "有限会社マックプランニング".into());
    let target_month = format!("{}年{:02}月", invoice.target_month.year(), invoice.target_month.month());
    let due_date = invoice.due_date.map(|d| d.format("%Y年%m月%d日").to_string()).unwrap_or_default();

    let status_and_actions = if invoice.client_accepted_at.is_none() {
        format!(
            r#"<div class="banner warn">まだ承諾されていません。請求書をご確認の上、下記ボタンより承諾をお願いいたします。</div>
<div class="actions">
  <a class="btn secondary" href="/token/{uuid}/pdf" target="_blank">請求書PDFを見る</a>
  <form method="POST" action="/token/{uuid}/accept" style="margin:0"><button type="submit" data-testid="accept-invoice-btn">内容を確認し、承諾する</button></form>
</div>"#,
            uuid = uuid
        )
    } else {
        let accepted = invoice.client_accepted_at.map(|d| d.format("%Y年%m月%d日 %H:%M").to_string()).unwrap_or_default();
        format!(
            r#"<div class="banner ok" data-testid="invoice-accepted-banner">この請求書は承諾済みです（{accepted}）。ご対応ありがとうございました。</div>
<div class="actions">
  <a class="btn secondary" href="/token/{uuid}/pdf" target="_blank">請求書PDFを見る</a>
</div>"#,
            accepted = accepted, uuid = uuid
        )
    };

    let body = format!(
        r#"<p>{client_name} 様</p>
{status_and_actions}
<h2>請求情報</h2>
<table>
  <tr><th>請求書番号</th><td>{invoice_id}</td></tr>
  <tr><th>対象月</th><td>{target_month}</td></tr>
  <tr><th>発行日</th><td>{issue_date}</td></tr>
  <tr><th>税込合計</th><td>¥{total}</td></tr>
  <tr><th>お支払期日</th><td>{due_date}</td></tr>
</table>
<p style="margin-top:24px;font-size:12px;color:#737373">{company_name}</p>"#,
        client_name = html_escape(&client_name),
        status_and_actions = status_and_actions,
        invoice_id = html_escape(&invoice.invoice_no),
        target_month = html_escape(&target_month),
        issue_date = invoice.issue_date.format("%Y年%m月%d日"),
        total = format_yen(invoice.total as i64),
        due_date = html_escape(&due_date),
        company_name = html_escape(&company_name),
    );

    page_shell(&format!("請求書 {} — Sophia", invoice.invoice_no), &body)
}

/// 支払通知書・代理請求書の確認ページ
async fn render_notice_page(pool: &PgPool, notice: &PaymentNotice, uuid: &str) -> String {
    let partner_name = order_repo::find_partner_name_email(pool, &notice.partner_id)
        .await.ok().flatten().map(|(n, _)| n).unwrap_or_default();
    let company_name = order_repo::find_company_name_setting(pool).await.unwrap_or_else(|_| "有限会社マックプランニング".into());
    let project_name = order_repo::find_project_name_by_purchase_order(pool, &notice.purchase_order_id)
        .await.ok().flatten().unwrap_or_default();
    let target_month = notice.target_month.format("%Y年%m月").to_string();

    let status_and_actions = if notice.partner_accepted_at.is_none() {
        format!(
            r#"<div class="banner warn">まだ承諾されていません。支払通知書と請求書をご確認の上、下記ボタンより承諾をお願いいたします。</div>
<div class="actions">
  <a class="btn secondary" href="/token/{uuid}/pdf" target="_blank">支払通知書PDFを見る</a>
  <a class="btn secondary" href="/token/{uuid}/invoice-pdf" target="_blank">請求書PDFを見る</a>
  <form method="POST" action="/token/{uuid}/accept" style="margin:0"><button type="submit">内容を確認し、承諾する</button></form>
</div>"#,
            uuid = uuid
        )
    } else {
        let finalized = notice.partner_accepted_at.map(|d| d.format("%Y年%m月%d日 %H:%M").to_string()).unwrap_or_default();
        format!(
            r#"<div class="banner ok">この支払通知書・請求書は承諾済みです（{finalized}）。ご対応ありがとうございました。</div>
<div class="actions">
  <a class="btn secondary" href="/token/{uuid}/pdf" target="_blank">支払通知書PDFを見る</a>
  <a class="btn secondary" href="/token/{uuid}/invoice-pdf" target="_blank">請求書PDFを見る</a>
</div>"#,
            finalized = finalized, uuid = uuid
        )
    };

    let body = format!(
        r#"<p>{partner_name} 様</p>
{status_and_actions}
<h2>支払通知・請求情報</h2>
<table>
  <tr><th>通知書番号</th><td>{notice_id}</td></tr>
  <tr><th>プロジェクト</th><td>{project_name}</td></tr>
  <tr><th>対象月</th><td>{target_month}</td></tr>
  <tr><th>通知日</th><td>{notice_date}</td></tr>
  <tr><th>税込合計</th><td>¥{total}</td></tr>
</table>
<p style="margin-top:16px;font-size:13px;color:#a3a3a3;">「請求書」は、貴社に代わり当社が作成した請求書です。承諾により貴社発行の請求書として受理します。</p>
<p style="margin-top:24px;font-size:12px;color:#737373">{company_name}</p>"#,
        partner_name = html_escape(&partner_name),
        notice_id = html_escape(&notice.notice_id),
        project_name = html_escape(&project_name),
        target_month = html_escape(&target_month),
        notice_date = notice.notice_date.format("%Y年%m月%d日"),
        total = format_yen(notice.total as i64),
        company_name = html_escape(&company_name),
    );

    page_shell(&format!("支払通知書 {} — Sophia", notice.notice_id), &body)
}

/// 注文書確認ページ本体を組み立てる
async fn render_order_page(
    pool: &PgPool,
    order: &PurchaseOrder,
    uuid: &str,
    upload_message: Option<(bool, String)>,
) -> String {
    let partner_name = order_repo::find_partner_name_email(pool, &order.partner_id)
        .await.ok().flatten().map(|(n, _)| n).unwrap_or_default();
    let project_name = order_repo::find_project_name(pool, &order.project_id)
        .await.ok().flatten().unwrap_or_default();
    let company_name = order_repo::find_company_name_setting(pool).await.unwrap_or_else(|_| "有限会社マックプランニング".into());

    let items = order_repo::list_order_items_with_engineer(pool, &order.order_id).await.unwrap_or_default();

    let mut items_html = String::new();
    let mut total: i64 = 0;
    for item in &items {
        total += item.amount as i64;
        items_html.push_str(&format!(
            "<tr><td>{}</td><td class=\"num\">¥{}</td><td class=\"num\">{}</td><td>{}〜{}h</td><td class=\"num\">¥{}</td></tr>",
            html_escape(&item.engineer_name), format_yen(item.base_fee as i64), item.effort,
            item.lower_limit_hours, item.upper_limit_hours, format_yen(item.amount as i64)
        ));
    }

    let empty = "―".to_string();
    let deliverable_text = if order.deliverable_text.is_empty() { &empty } else { &order.deliverable_text };
    let payment_condition = if order.payment_condition.is_empty() { &empty } else { &order.payment_condition };
    let work_location = if order.work_location.is_empty() { &empty } else { &order.work_location };

    let status_and_actions = if order.status == "SENT" {
        format!(
            r#"<div class="banner warn">まだ承諾されていません。内容をご確認の上、下記ボタンより承諾をお願いいたします。</div>
<div class="actions">
  <a class="btn secondary" href="/token/{uuid}/pdf">注文書PDFを見る</a>
  <form method="POST" action="/token/{uuid}/accept" style="margin:0"><button type="submit">内容を確認し、承諾する</button></form>
</div>"#,
            uuid = uuid
        )
    } else {
        let finalized = order.partner_accepted_at.map(|d| d.format("%Y年%m月%d日 %H:%M").to_string()).unwrap_or_default();
        format!(
            r#"<div class="banner ok">この注文書は承諾済みです（{finalized}）。ご対応ありがとうございました。</div>
<div class="actions">
  <a class="btn secondary" href="/token/{uuid}/pdf">注文書PDFを見る</a>
  <a class="btn secondary" href="/token/{uuid}/acceptance-pdf">注文請書PDFを見る</a>
</div>
<h2>稼働報告書の提出</h2>
<p>作業月分の稼働報告書（Excel/PDF）をアップロードしてください。</p>
<form method="POST" action="/token/{uuid}/timesheet" enctype="multipart/form-data" style="margin:0">
  <div id="dropzone" style="border:2px dashed #404040;border-radius:8px;padding:28px 16px;text-align:center;cursor:pointer;margin-bottom:12px;transition:border-color .15s,background .15s;">
    <p id="dropzone-text" style="margin:0;color:#a3a3a3;font-size:13px;">ここにファイルをドラッグ＆ドロップ、またはクリックして選択</p>
    <input type="file" name="file" id="file-input" accept=".xlsx,.xls,.pdf" required style="display:none">
  </div>
  <button type="submit">稼働報告書を提出する</button>
</form>
<script>
(function() {{
  var dz = document.getElementById('dropzone');
  var input = document.getElementById('file-input');
  var text = document.getElementById('dropzone-text');
  dz.addEventListener('click', function() {{ input.click(); }});
  dz.addEventListener('dragover', function(e) {{
    e.preventDefault();
    dz.style.borderColor = '#10b981';
    dz.style.background = 'rgba(16,185,129,0.08)';
  }});
  dz.addEventListener('dragleave', function() {{
    dz.style.borderColor = '#404040';
    dz.style.background = 'transparent';
  }});
  dz.addEventListener('drop', function(e) {{
    e.preventDefault();
    dz.style.borderColor = '#404040';
    dz.style.background = 'transparent';
    if (e.dataTransfer.files.length > 0) {{
      input.files = e.dataTransfer.files;
      text.textContent = e.dataTransfer.files[0].name;
    }}
  }});
  input.addEventListener('change', function() {{
    if (input.files.length > 0) text.textContent = input.files[0].name;
  }});
}})();
</script>"#,
            finalized = finalized, uuid = uuid
        )
    };

    let upload_banner = match upload_message {
        Some((true, msg)) => format!(r#"<div class="banner ok">{}</div>"#, html_escape(&msg)),
        Some((false, msg)) => format!(r#"<div class="banner warn">{}</div>"#, html_escape(&msg)),
        None => String::new(),
    };

    let body = format!(
        r#"<p>{partner_name} 様</p>
{upload_banner}
{status_and_actions}
<h2>注文書情報</h2>
<table>
  <tr><th>注文番号</th><td>{order_id}</td></tr>
  <tr><th>プロジェクト</th><td>{project_name}</td></tr>
  <tr><th>発注日</th><td>{order_date}</td></tr>
  <tr><th>作業期間</th><td>{work_start} 〜 {work_end}</td></tr>
  <tr><th>作業場所</th><td>{work_location}</td></tr>
  <tr><th>納入物件</th><td>{deliverable_text}</td></tr>
  <tr><th>支払条件</th><td>{payment_condition}</td></tr>
</table>
<h2>発注明細</h2>
<table class="items">
  <tr><th>要員</th><th class="num">単価</th><th class="num">工数</th><th>精算幅</th><th class="num">金額</th></tr>
  {items_html}
  <tr><th colspan="4" style="text-align:right">合計</th><td class="num"><strong>¥{total}</strong></td></tr>
</table>
<p style="margin-top:24px;font-size:12px;color:#737373">{company_name}</p>"#,
        partner_name = html_escape(&partner_name),
        order_id = html_escape(&order.order_id),
        project_name = html_escape(&project_name),
        order_date = order.order_date.format("%Y年%m月%d日"),
        work_start = order.work_start.format("%Y年%m月%d日"),
        work_end = order.work_end.format("%Y年%m月%d日"),
        work_location = html_escape(work_location),
        deliverable_text = html_escape(deliverable_text),
        payment_condition = html_escape(payment_condition),
        total = format_yen(total),
        company_name = html_escape(&company_name),
    );

    page_shell(&format!("注文書 {} — Sophia", order.order_id), &body)
}

fn format_yen(n: i64) -> String {
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    if n < 0 { format!("-{}", formatted) } else { formatted }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 承諾処理（POST /token/:uuid/accept）
///
/// パートナーがトークンURLで承諾した時の処理。
/// EDIと同様に、承諾後に自社担当者へ通知メールを送信する。
pub async fn accept(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    use axum::response::Html;
    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    // 注文書の承諾
    let order_result = order_repo::accept_order_by_uuid(&pool, &uuid_parsed).await;

    if let Ok(rows) = order_result {
        if rows > 0 {
            send_order_approve_notification(&pool, uuid_parsed).await;
            return Redirect::to(&format!("/token/{}", uuid)).into_response();
        }
    }

    // 支払通知書の承諾（代理請求書PDFを保存してハッシュ記録）
    match order_repo::find_payment_notice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(notice)) => {
            if notice.partner_accepted_at.is_none() {
                use sha2::{Digest, Sha256};

                let pdf_bytes = super::notices::generate_partner_invoice_pdf_bytes(&pool, &notice).await.unwrap_or_default();
                let mut hasher = Sha256::new();
                hasher.update(&pdf_bytes);
                let document_hash = format!("{:x}", hasher.finalize());

                if !pdf_bytes.is_empty() {
                    let pdf_dir = std::path::Path::new("uploads/invoices");
                    let _ = std::fs::create_dir_all(pdf_dir);
                    let _ = std::fs::write(pdf_dir.join(format!("invoice_{}.pdf", notice.notice_id)), &pdf_bytes);
                }

                if let Err(e) = order_repo::confirm_payment_notice(&pool, &notice.notice_id, &document_hash).await {
                    tracing::error!("支払通知承諾DB更新エラー: {:?}", e);
                } else {
                    if let Err(e) = order_repo::update_purchase_order_status(&pool, &notice.purchase_order_id, "NOTICE_CONFIRMED").await {
                        tracing::error!("注文書ステータス更新エラー: {:?}", e);
                    }
                    send_invoice_approve_notification(&pool, uuid_parsed).await;
                }
            }
            return Redirect::to(&format!("/token/{}", uuid)).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンリンク解決] 処理=支払通知書検索(承諾) 影響=承諾ページを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Html(error_page(
                "お手数ですが、しばらく時間をおいてからアクセスしてください。"
            ))).into_response();
        }
    }

    // クライアント向け請求書の承諾
    use crate::infrastructure::repositories::billing_repo;
    match billing_repo::find_invoice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(invoice)) => {
            if invoice.client_accepted_at.is_none() {
                use sha2::{Digest, Sha256};
                use crate::domain::services::pdf_generator::PdfGenerator;

                let client_name = order_repo::find_client_name(&pool, invoice.client_id).await.unwrap_or_default();
                let pdf_data = super::invoices::build_client_invoice_pdf_data(&pool, &invoice, &client_name).await;
                let gen = PdfGenerator::new();
                let pdf_bytes = gen.generate_invoice_pdf(&pdf_data).unwrap_or_default();

                let mut hasher = Sha256::new();
                hasher.update(&pdf_bytes);
                let document_hash = format!("{:x}", hasher.finalize());

                if !pdf_bytes.is_empty() {
                    let pdf_dir = std::path::Path::new("uploads/invoices");
                    let _ = std::fs::create_dir_all(pdf_dir);
                    let _ = std::fs::write(pdf_dir.join(format!("invoice_{}.pdf", invoice.invoice_no)), &pdf_bytes);
                }

                if let Err(e) = billing_repo::confirm_client_invoice(&pool, invoice.id, &document_hash).await {
                    tracing::error!("請求書承諾DB更新エラー: {:?}", e);
                } else {
                    if let Some(ro_id) = invoice.received_order_id {
                        if let Err(e) = order_repo::mark_received_order_invoice_confirmed(&pool, ro_id).await {
                            tracing::error!("受注ステータス更新エラー: {:?}", e);
                        }
                    }
                    // 承諾通知メール（将来実装）
                }
            }
            return Redirect::to(&format!("/token/{}", uuid)).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンリンク解決] 処理=請求書検索(承諾) 影響=承諾ページを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Html(error_page(
                "お手数ですが、しばらく時間をおいてからアクセスしてください。"
            ))).into_response();
        }
    }

    axum::http::StatusCode::NOT_FOUND.into_response()
}

/// クエリパラメータ用にメッセージをパーセントエンコードする
fn encode_query_message(msg: &str) -> String {
    url::form_urlencoded::byte_serialize(msg.as_bytes()).collect()
}

/// 稼働報告アップロード（POST /token/:uuid/timesheet）
///
/// 認証不要。トークンに紐づく発注書のパートナー（`order.partner_id`）配下で
/// アップロードファイルの作業者名から受注契約を解決する。トークン自身の
/// `partner_contract_id` へ強制的に紐付けない（1パートナーに複数エンジニアが
/// いる場合の誤紐付けを避けるため。認証済みの `/api/portal/timesheets/upload` と
/// 同じ解決ロジック）。
pub async fn upload_timesheet(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    use axum::response::Html;
    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => return Redirect::to(&format!("/token/{}?upload_error={}", uuid, encode_query_message("無効なリンクです"))).into_response(),
    };

    let order = match order_repo::find_purchase_order_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(o)) => o,
        Ok(None) => return axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("[トークン 稼働報告アップロード] 処理=注文書検索 影響=アップロード処理失敗 | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Html(error_page(
                "お手数ですが、しばらく時間をおいてからアクセスしてください。"
            ))).into_response();
        }
    };

    let redirect_with = |ok: bool, msg: String| -> axum::response::Response {
        let param = if ok { "uploaded" } else { "upload_error" };
        Redirect::to(&format!("/token/{}?{}={}", uuid, param, encode_query_message(&msg))).into_response()
    };

    let (bytes, original_filename) = match timesheet_upload_common::extract_upload_file(&mut multipart).await {
        Ok(v) => v,
        Err(e) => return redirect_with(false, extract_error_message(&e)),
    };

    let result = match timesheet_upload_common::parse_upload(&bytes, &original_filename) {
        Ok(r) => r,
        Err(e) => return redirect_with(false, extract_error_message(&e)),
    };

    let target_month = timesheet_upload_common::normalize_target_month(&result);

    let contract_id = match timesheet_upload_common::resolve_partner_contract_id(
        &pool, &order.partner_id, &result.worker_name, target_month, 0,
    ).await {
        Ok(id) => id,
        Err(e) => return redirect_with(false, extract_error_message(&e)),
    };

    if let Err(e) = timesheet_upload_common::ensure_received_order(
        &pool, contract_id, target_month, &result.worker_name,
    ).await {
        return redirect_with(false, extract_error_message(&e));
    }

    match timesheet_upload_common::upsert_parsed_timesheet(
        &pool, contract_id, target_month, &result, &original_filename, "UPLOADED",
    ).await {
        Ok(()) => {
            tracing::info!(
                "トークン経由 稼働報告アップロード: order={} → {}h / {}日",
                order.order_id, result.total_hours, result.work_days
            );
            redirect_with(true, format!("{} を受け付けました。ありがとうございました。", original_filename))
        }
        Err(e) => redirect_with(false, extract_error_message(&e)),
    }
}

/// TimesheetUploadError の JSON から表示用メッセージ文字列を取り出す
fn extract_error_message(e: &timesheet_upload_common::TimesheetUploadError) -> String {
    e.to_json()
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("アップロードに失敗しました")
        .to_string()
}

/// 注文書承諾時の社内通知メール送信
async fn send_order_approve_notification(pool: &PgPool, uuid: uuid::Uuid) {
    use crate::domain::services::email_service::{EmailService, compose_order_approve_email};

    let order_info = order_repo::find_order_id_partner_info_by_uuid(pool, &uuid).await.ok().flatten();

    let (order_id, partner_name, _partner_email, project_name, _work_start, _work_end) = match order_info {
        Some(info) => info,
        None => return,
    };

    let email_svc = EmailService::new(pool.clone());
    let notify_email = crate::domain::services::email_service::get_notify_email(pool).await;

    let ctx = compose_order_approve_email(&partner_name, &order_id, &project_name);
    if let Err(e) = email_svc.send_by_template("order_approve", &notify_email, None, &ctx).await {
        tracing::error!("注文書承諾通知メール送信エラー: {:?}", e);
    } else {
        tracing::info!("[注文承諾通知] {} が {} を承諾 → {} に通知", partner_name, order_id, notify_email);
    }
}

/// 支払通知書承諾時の社内通知メール送信
async fn send_invoice_approve_notification(pool: &PgPool, uuid: uuid::Uuid) {
    use crate::domain::services::email_service::{EmailService, compose_invoice_approve_email};

    let notice_info = order_repo::find_notice_id_partner_name_by_uuid(pool, &uuid).await.ok().flatten();

    let (notice_id, partner_name, target_month, total) = match notice_info {
        Some(info) => info,
        None => return,
    };

    let email_svc = EmailService::new(pool.clone());
    let notify_email = crate::domain::services::email_service::get_notify_email(pool).await;

    let ctx = compose_invoice_approve_email(&partner_name, &notice_id, &target_month.format("%Y年%m月").to_string(), total as i64);
    if let Err(e) = email_svc.send_by_template("invoice_approve", &notify_email, None, &ctx).await {
        tracing::error!("支払通知書承諾通知メール送信エラー: {:?}", e);
    } else {
        tracing::info!("[支払通知書承諾通知] {} が {} を承諾 → {} に通知", partner_name, notice_id, notify_email);
    }
}

/// クライアント請求書 受領確認時の社内通知メール送信
async fn send_invoice_receipt_confirmed_notification(
    pool: &PgPool,
    invoice: &crate::domain::models::billing::BillingInvoice,
    client_name: &str,
) {
    use crate::domain::services::email_service::{EmailService, compose_invoice_receipt_confirmed_email};
    use chrono::Datelike;

    let email_svc = EmailService::new(pool.clone());
    let notify_email = crate::domain::services::email_service::get_notify_email(pool).await;

    let ctx = compose_invoice_receipt_confirmed_email(
        client_name,
        &invoice.invoice_no,
        &format!("{}年{:02}月", invoice.target_month.year(), invoice.target_month.month()),
        invoice.total as i64,
    );
    if let Err(e) = email_svc.send_by_template("client_invoice_receipt_confirmed", &notify_email, None, &ctx).await {
        tracing::error!("請求書受領確認通知メール送信エラー: {:?}", e);
    } else {
        tracing::info!("[請求書受領確認] {} が {} を確認 → {} に通知", client_name, invoice.invoice_no, notify_email);
    }
}

/// PDFダウンロード（GET /token/:uuid/pdf）
pub async fn download_pdf(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    match order_repo::find_purchase_order_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(o)) => {
            if is_token_expired(&pool, o.token_issued_at).await {
                return StatusCode::FORBIDDEN.into_response();
            }
            match super::orders::build_purchase_order_pdf_data(&pool, &o).await {
                Ok(pdf_data) => match PdfGenerator::new().generate_purchase_order_pdf(&pdf_data) {
                    Ok(bytes) => return Response::builder()
                        .header(header::CONTENT_TYPE, "application/pdf")
                        .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"order_{}.pdf\"", o.order_id))
                        .body(Body::from(bytes)).expect("PDF response").into_response(),
                    Err(e) => { tracing::error!("PDF生成エラー: {}", e); return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
                },
                Err(e) => { tracing::error!("PDFデータ構築エラー: {}", e); return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークン PDF取得] 処理=注文書検索 影響=PDF取得失敗 | {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    match order_repo::find_payment_notice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(n)) => {
            match super::notices::generate_payment_notice_pdf_bytes(&pool, &n).await {
                Ok(bytes) => return Response::builder()
                    .header(header::CONTENT_TYPE, "application/pdf")
                    .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"notice_{}.pdf\"", n.notice_id))
                    .body(Body::from(bytes)).expect("PDF response").into_response(),
                Err(e) => { tracing::error!("{}", e); return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークン PDF取得] 処理=支払通知書検索 影響=PDF取得失敗 | {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    use crate::infrastructure::repositories::billing_repo;
    match billing_repo::find_invoice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(invoice)) => {
            if is_token_expired(&pool, invoice.sent_at).await {
                return StatusCode::FORBIDDEN.into_response();
            }
            let client_name = order_repo::find_client_name(&pool, invoice.client_id).await.unwrap_or_default();
            let pdf_data = super::invoices::build_client_invoice_pdf_data(&pool, &invoice, &client_name).await;
            return match PdfGenerator::new().generate_invoice_pdf(&pdf_data) {
                Ok(bytes) => Response::builder()
                    .header(header::CONTENT_TYPE, "application/pdf")
                    .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"invoice_{}.pdf\"", invoice.invoice_no))
                    .body(Body::from(bytes)).expect("PDF response").into_response(),
                Err(e) => { tracing::error!("請求書PDF生成エラー: {}", e); StatusCode::INTERNAL_SERVER_ERROR.into_response() }
            };
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークン PDF取得] 処理=請求書検索 影響=PDF取得失敗 | {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

/// パートナー代理請求書PDF（GET /token/:uuid/invoice-pdf）
pub async fn download_invoice_pdf(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};

    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let notice = match order_repo::find_payment_notice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(n)) => n,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("[トークン 代理請求書PDF取得] 処理=支払通知書検索 影響=PDF取得失敗 | {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match super::notices::generate_partner_invoice_pdf_bytes(&pool, &notice).await {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, "application/pdf")
            .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"invoice_{}.pdf\"", notice.notice_id))
            .body(Body::from(bytes)).expect("PDF response").into_response(),
        Err(e) => {
            tracing::error!("{}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// 注文請書PDFダウンロード（GET /token/:uuid/acceptance-pdf）
pub async fn download_acceptance_pdf(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let order = match order_repo::find_purchase_order_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(o)) => {
            if is_token_expired(&pool, o.token_issued_at).await {
                return StatusCode::FORBIDDEN.into_response();
            }
            o
        }
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("[トークン 注文請書PDF取得] 処理=注文書検索 影響=PDF取得失敗 | {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    match super::orders::build_purchase_order_pdf_data(&pool, &order).await {
        Ok(pdf_data) => match PdfGenerator::new().generate_acceptance_pdf(&pdf_data) {
            Ok(bytes) => Response::builder()
                .header(header::CONTENT_TYPE, "application/pdf")
                .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"acceptance_{}.pdf\"", order.order_id))
                .body(Body::from(bytes)).expect("PDF response").into_response(),
            Err(e) => { tracing::error!("注文請書PDF生成エラー: {}", e); StatusCode::INTERNAL_SERVER_ERROR.into_response() }
        },
        Err(e) => { tracing::error!("PDFデータ構築エラー: {}", e); StatusCode::INTERNAL_SERVER_ERROR.into_response() }
    }
}

// ── §14 JSON API（Next.js トークンページ用）──

/// GET /api/v1/token/{uuid} — トークン文書の JSON
pub async fn api_view(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "無効なリンクです" })),
            )
                .into_response();
        }
    };

    match order_repo::find_purchase_order_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(order)) => {
            if order.status == "SENT" && is_token_expired(&pool, order.token_issued_at).await {
                return (
                    axum::http::StatusCode::GONE,
                    Json(serde_json::json!({ "error": "このリンクの有効期限が切れています" })),
                )
                    .into_response();
            }
            let partner_name = order_repo::find_partner_name_email(&pool, &order.partner_id)
                .await.ok().flatten().map(|(n, _)| n).unwrap_or_default();
            let project_name = order_repo::find_project_name(&pool, &order.project_id)
                .await.ok().flatten().unwrap_or_default();
            let company_name = order_repo::find_company_name_setting(&pool).await.unwrap_or_else(|_| "有限会社マックプランニング".into());
            let items = order_repo::list_order_items_with_engineer(&pool, &order.order_id).await.unwrap_or_default();
            let items_json: Vec<_> = items.iter().map(|item| serde_json::json!({
                "engineer_name": item.engineer_name,
                "base_fee": item.base_fee,
                "effort": item.effort,
                "lower_limit_hours": item.lower_limit_hours,
                "upper_limit_hours": item.upper_limit_hours,
                "price": item.amount,
            })).collect();
            let accepted = order.status != "SENT";
            let can_upload_timesheet = accepted && chrono::Local::now().date_naive() >= order.work_start;
            return Json(serde_json::json!({
                "kind": "order",
                "uuid": uuid,
                "partner_name": partner_name,
                "project_name": project_name,
                "company_name": company_name,
                "order_id": order.order_id,
                "status": order.status,
                "accepted": accepted,
                "finalized_at": order.partner_accepted_at.map(|d| d.format("%Y年%m月%d日 %H:%M").to_string()),
                "deliverable_text": order.deliverable_text,
                "payment_condition": order.payment_condition,
                "work_location": order.work_location,
                "items": items_json,
                "pdf": {
                    "order": format!("/api/v1/token/{}/pdf", uuid),
                    "acceptance": if accepted { Some(format!("/api/v1/token/{}/acceptance-pdf", uuid)) } else { None },
                },
                "can_accept": !accepted,
                "can_upload_timesheet": can_upload_timesheet,
            })).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンAPI] 処理=注文書検索 影響=確認APIを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "サーバーエラーが発生しました" }))).into_response();
        }
    }

    match order_repo::find_payment_notice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(notice)) => {
            let partner_name = order_repo::find_partner_name_email(&pool, &notice.partner_id)
                .await.ok().flatten().map(|(n, _)| n).unwrap_or_default();
            let company_name = order_repo::find_company_name_setting(&pool).await.unwrap_or_else(|_| "有限会社マックプランニング".into());
            let project_name = order_repo::find_project_name_by_purchase_order(&pool, &notice.purchase_order_id)
                .await.ok().flatten().unwrap_or_default();
            let accepted = notice.partner_accepted_at.is_some();
            return Json(serde_json::json!({
                "kind": "notice",
                "uuid": uuid,
                "partner_name": partner_name,
                "project_name": project_name,
                "company_name": company_name,
                "notice_id": notice.notice_id,
                "target_month": notice.target_month.format("%Y年%m月").to_string(),
                "notice_date": notice.notice_date.format("%Y年%m月%d日").to_string(),
                "total": notice.total,
                "accepted": accepted,
                "confirmed_at": notice.partner_accepted_at.map(|d| d.format("%Y年%m月%d日 %H:%M").to_string()),
                "pdf": {
                    "notice": format!("/api/v1/token/{}/pdf", uuid),
                    "invoice": format!("/api/v1/token/{}/invoice-pdf", uuid),
                },
                "can_accept": !accepted,
            })).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンAPI] 処理=支払通知書検索 影響=確認APIを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "サーバーエラーが発生しました" }))).into_response();
        }
    }

    use crate::infrastructure::repositories::billing_repo;
    match billing_repo::find_invoice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(invoice)) => {
            if is_token_expired(&pool, invoice.sent_at).await {
                return (
                    axum::http::StatusCode::GONE,
                    Json(serde_json::json!({ "error": "このリンクの有効期限が切れています" })),
                )
                    .into_response();
            }
            use chrono::Datelike;
            let client_name = order_repo::find_client_name(&pool, invoice.client_id).await.unwrap_or_default();
            let company_name = order_repo::find_company_name_setting(&pool).await.unwrap_or_else(|_| "有限会社マックプランニング".into());
            let accepted = invoice.client_accepted_at.is_some();
            return Json(serde_json::json!({
                "kind": "invoice",
                "uuid": uuid,
                "client_name": client_name,
                "company_name": company_name,
                "invoice_no": invoice.invoice_no,
                "target_month": format!("{}年{:02}月", invoice.target_month.year(), invoice.target_month.month()),
                "issue_date": invoice.issue_date.format("%Y年%m月%d日").to_string(),
                "due_date": invoice.due_date.map(|d| d.format("%Y年%m月%d日").to_string()),
                "total": invoice.total,
                "accepted": accepted,
                "confirmed_at": invoice.client_accepted_at.map(|d| d.format("%Y年%m月%d日 %H:%M").to_string()),
                "pdf": { "invoice": format!("/api/v1/token/{}/pdf", uuid) },
                "can_accept": !accepted,
            })).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンAPI] 処理=請求書検索 影響=確認APIを開けない | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "サーバーエラーが発生しました" }))).into_response();
        }
    }

    (
        axum::http::StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "リンクが見つかりません" })),
    )
        .into_response()
}

/// POST /api/v1/token/{uuid}/accept — 承諾（JSON）
pub async fn api_accept(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
) -> impl IntoResponse {
    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": "無効なリンクです" })),
            )
                .into_response();
        }
    };

    if let Ok(rows) = order_repo::accept_order_by_uuid(&pool, &uuid_parsed).await {
        if rows > 0 {
            send_order_approve_notification(&pool, uuid_parsed).await;
            return Json(serde_json::json!({ "success": true })).into_response();
        }
    }

    match order_repo::find_payment_notice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(notice)) => {
            if notice.partner_accepted_at.is_none() {
                use sha2::{Digest, Sha256};
                let pdf_bytes = super::notices::generate_partner_invoice_pdf_bytes(&pool, &notice).await.unwrap_or_default();
                let mut hasher = Sha256::new();
                hasher.update(&pdf_bytes);
                let document_hash = format!("{:x}", hasher.finalize());
                if !pdf_bytes.is_empty() {
                    let pdf_dir = std::path::Path::new("uploads/invoices");
                    let _ = std::fs::create_dir_all(pdf_dir);
                    let _ = std::fs::write(pdf_dir.join(format!("invoice_{}.pdf", notice.notice_id)), &pdf_bytes);
                }
                if let Err(e) = order_repo::confirm_payment_notice(&pool, &notice.notice_id, &document_hash).await {
                    tracing::error!("支払通知承諾DB更新エラー: {:?}", e);
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "success": false, "error": "承諾処理に失敗しました" })),
                    )
                        .into_response();
                }
                if let Err(e) = order_repo::update_purchase_order_status(&pool, &notice.purchase_order_id, "NOTICE_CONFIRMED").await {
                    tracing::error!("注文書ステータス更新エラー: {:?}", e);
                }
                send_invoice_approve_notification(&pool, uuid_parsed).await;
            }
            return Json(serde_json::json!({ "success": true })).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンAPI 承諾] 処理=支払通知書検索 影響=承諾処理失敗 | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "error": "サーバーエラーが発生しました" }))).into_response();
        }
    }

    use crate::infrastructure::repositories::billing_repo;
    match billing_repo::find_invoice_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(invoice)) => {
            if invoice.client_accepted_at.is_none() {
                use sha2::{Digest, Sha256};
                use crate::domain::services::pdf_generator::PdfGenerator;

                let client_name = order_repo::find_client_name(&pool, invoice.client_id).await.unwrap_or_default();
                let pdf_data = super::invoices::build_client_invoice_pdf_data(&pool, &invoice, &client_name).await;
                let gen = PdfGenerator::new();
                let pdf_bytes = gen.generate_invoice_pdf(&pdf_data).unwrap_or_default();

                let mut hasher = Sha256::new();
                hasher.update(&pdf_bytes);
                let document_hash = format!("{:x}", hasher.finalize());

                if !pdf_bytes.is_empty() {
                    let pdf_dir = std::path::Path::new("uploads/invoices");
                    let _ = std::fs::create_dir_all(pdf_dir);
                    let _ = std::fs::write(pdf_dir.join(format!("invoice_{}.pdf", invoice.invoice_no)), &pdf_bytes);
                }

                if let Err(e) = billing_repo::confirm_client_invoice(&pool, invoice.id, &document_hash).await {
                    tracing::error!("請求書受領確認DB更新エラー: {:?}", e);
                    return (
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({ "success": false, "error": "受領確認処理に失敗しました" })),
                    ).into_response();
                }
                if let Some(ro_id) = invoice.received_order_id {
                    if let Err(e) = order_repo::mark_received_order_invoice_confirmed(&pool, ro_id).await {
                        tracing::error!("受注ステータス更新エラー: {:?}", e);
                    }
                }
                send_invoice_receipt_confirmed_notification(&pool, &invoice, &client_name).await;
            }
            return Json(serde_json::json!({ "success": true })).into_response();
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("[トークンAPI 承諾] 処理=請求書検索 影響=承諾処理失敗 | {}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "error": "サーバーエラーが発生しました" }))).into_response();
        }
    }

    (
        axum::http::StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "success": false, "error": "リンクが見つかりません" })),
    )
        .into_response()
}

/// POST /api/v1/token/{uuid}/timesheet — 稼働報告アップロード（JSON）
pub async fn api_upload_timesheet(
    State(pool): State<PgPool>,
    Path(uuid): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let uuid_parsed = match uuid::Uuid::parse_str(&uuid) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": "無効なリンクです" })),
            )
                .into_response();
        }
    };

    let order = match order_repo::find_purchase_order_by_uuid(&pool, &uuid_parsed).await {
        Ok(Some(o)) => o,
        Ok(None) => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "success": false, "error": "注文書が見つかりません" })),
            )
                .into_response();
        }
        Err(e) => {
            tracing::error!("[トークンAPI アップロード] 処理=注文書検索 影響=アップロード処理失敗 | {}", e);
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "success": false, "error": "サーバーエラーが発生しました" })),
            )
                .into_response();
        }
    };

    let (bytes, original_filename) = match timesheet_upload_common::extract_upload_file(&mut multipart).await {
        Ok(v) => v,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": extract_error_message(&e) })),
            )
                .into_response();
        }
    };

    let result = match timesheet_upload_common::parse_upload(&bytes, &original_filename) {
        Ok(r) => r,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": extract_error_message(&e) })),
            )
                .into_response();
        }
    };

    let target_month = timesheet_upload_common::normalize_target_month(&result);
    let contract_id = match timesheet_upload_common::resolve_partner_contract_id(
        &pool, &order.partner_id, &result.worker_name, target_month, 0,
    ).await {
        Ok(id) => id,
        Err(e) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": extract_error_message(&e) })),
            )
                .into_response();
        }
    };

    if let Err(e) = timesheet_upload_common::ensure_received_order(
        &pool, contract_id, target_month, &result.worker_name,
    ).await {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "error": extract_error_message(&e) })),
        )
            .into_response();
    }

    match timesheet_upload_common::upsert_parsed_timesheet(
        &pool, contract_id, target_month, &result, &original_filename, "UPLOADED",
    ).await {
        Ok(()) => {
            tracing::info!(
                "トークン経由 稼働報告アップロード(API): order={} → {}h / {}日",
                order.order_id, result.total_hours, result.work_days
            );
            Json(serde_json::json!({
                "success": true,
                "message": format!("{} を受け付けました。ありがとうございました。", original_filename)
            }))
            .into_response()
        }
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "success": false, "error": extract_error_message(&e) })),
        )
            .into_response(),
    }
}
