/// infrastructure/repositories/billing_repo.rs — 請求書CRUD
///
/// list_invoices, find_invoice は base.rs マクロで自動生成。
/// list_invoice_items, list_payments はFK子レコード取得マクロで生成。

use crate::domain::models::billing::*;

// ── 共通CRUD（マクロ生成）──

impl_list_all!(list_invoices, BillingInvoice, "t_billing_invoice", "created_at DESC");
impl_find_by_id!(find_invoice, BillingInvoice, "t_billing_invoice", "id");

/// 請求書取得（UUID指定 — クライアント向けトークンアクセス用）
pub async fn find_invoice_by_uuid(pool: &::sqlx::PgPool, uuid: &::uuid::Uuid) -> ::anyhow::Result<Option<BillingInvoice>> {
    let row = ::sqlx::query_as::<_, BillingInvoice>(
        "SELECT * FROM t_billing_invoice WHERE uuid = $1"
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// このクライアントへ過去に送信済み（sent_at IS NOT NULL）の請求書が存在するか。
/// 初回送信メール（client_invoice_send_first）と2回目以降（client_invoice_send）の
/// テンプレート出し分けに使う。
pub async fn has_prior_sent_invoice(pool: &::sqlx::PgPool, client_id: i64, exclude_id: i64) -> ::anyhow::Result<bool> {
    let row: (bool,) = ::sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM t_billing_invoice WHERE client_id = $1 AND id != $2 AND sent_at IS NOT NULL)"
    )
    .bind(client_id)
    .bind(exclude_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

// ── FK子レコード取得（マクロ生成）──
//
// 2026-07-12修正: テーブル名が旧名 "t_billing_item" のままだった
// （007/008で "t_billing_invoice_item" に改名済み）。加えて存在しない
// "sort_order" カラムを参照していた。このリポジトリ層はどのハンドラからも
// 呼ばれていない未使用コードだったため、これまで実行時エラーとして
// 顕在化していなかった（P2-3調査で発見）。

impl_list_by_fk!(list_invoice_items, BillingItem, "t_billing_invoice_item", "invoice_id", "id");
impl_list_by_fk!(list_payments, PaymentRecord, "t_payment_record", "invoice_id", "payment_date");

// ── 請求書承認ワークフロー（2026-07-12追加。P2-3: handlers直書きSQLの
//    Repository層移行の第一弾。invoices.rs::api_approve/api_reject から利用）──

/// 承認待ち(PENDING_APPROVAL)または差戻し(REJECTED)の請求書を承認済み(APPROVED)にする。
/// 対象が承認待ちでなければ何もせずfalseを返す（楽観ロック的なガード）。
pub async fn approve_invoice(pool: &::sqlx::PgPool, id: i64, approved_by_id: i64) -> ::anyhow::Result<bool> {
    let result = ::sqlx::query(
        "UPDATE t_billing_invoice SET status = 'APPROVED', approved_by_id = $1, approved_at = NOW(), updated_at = NOW() WHERE id = $2 AND status IN ('PENDING_APPROVAL', 'REJECTED')"
    )
    .bind(approved_by_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 承認待ち(PENDING_APPROVAL)または承認済み(APPROVED)の請求書を差戻し(REJECTED)にする。
/// 対象がそれ以外（差戻し済み・送信済み）なら何もせずfalseを返す。
pub async fn reject_invoice(pool: &::sqlx::PgPool, id: i64) -> ::anyhow::Result<bool> {
    let result = ::sqlx::query(
        "UPDATE t_billing_invoice SET status = 'REJECTED', approved_by_id = NULL, approved_at = NULL, updated_at = NOW() WHERE id = $1 AND status IN ('PENDING_APPROVAL', 'APPROVED')"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

// ── 請求書PDF一括生成（2026-07-13追加。P2-3続き:
//    presentation/handlers/home.rs 直書きSQLのRepository層移行）──

/// invoice_pdfが未生成の請求書一覧（PDF一括生成バッチ用）
pub async fn list_invoices_without_pdf(pool: &::sqlx::PgPool) -> ::anyhow::Result<Vec<BillingInvoice>> {
    let rows = ::sqlx::query_as::<_, BillingInvoice>(
        "SELECT * FROM t_billing_invoice WHERE COALESCE(invoice_pdf, '') = '' ORDER BY id"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 請求明細（PDF出力用: 内容, 数量, 単価, 金額）
pub async fn find_invoice_items_for_pdf(pool: &::sqlx::PgPool, invoice_id: i64) -> ::anyhow::Result<Vec<(String, String, i64, i64)>> {
    let rows: Vec<(String, String, i64, i64)> = ::sqlx::query_as(
        "SELECT description, COALESCE(quantity::text, '1.0'), unit_price::bigint, amount::bigint \
         FROM t_billing_invoice_item WHERE invoice_id = $1 ORDER BY id"
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 自社情報（請求書PDF用: 会社情報＋銀行口座情報）
#[derive(Default, ::sqlx::FromRow)]
pub struct CompanyInvoiceInfo {
    pub name: String,
    pub address: String,
    pub tel: String,
    pub postal_code: String,
    pub fax: String,
    pub representative_title: String,
    pub representative_name: String,
    pub registration_number: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
}

/// 自社情報・銀行口座情報を取得する（請求書PDF用）
pub async fn find_company_invoice_info(pool: &::sqlx::PgPool) -> ::anyhow::Result<Option<CompanyInvoiceInfo>> {
    let row = ::sqlx::query_as::<_, CompanyInvoiceInfo>(
        "SELECT name, address, tel, COALESCE(postal_code, '') AS postal_code, COALESCE(fax, '') AS fax, \
         COALESCE(representative_title, '') AS representative_title, COALESCE(representative_name, '') AS representative_name, \
         COALESCE(registration_no, '') AS registration_number, \
         COALESCE(bank_name, '') AS bank_name, COALESCE(bank_branch, '') AS bank_branch, \
         COALESCE(account_type, '') AS account_type, COALESCE(account_number, '') AS account_number, \
         COALESCE(account_name, '') AS account_name \
         FROM s_company_info ORDER BY id LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 請求書PDFのDrive URL・ファイルIDを保存する
pub async fn set_invoice_pdf(pool: &::sqlx::PgPool, id: i64, drive_url: &str, drive_file_id: &str) -> ::anyhow::Result<()> {
    ::sqlx::query(
        "UPDATE t_billing_invoice SET invoice_pdf = $1, drive_file_id = $2 WHERE id = $3"
    )
    .bind(drive_url)
    .bind(drive_file_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── 請求書の編集・送付先（2026-07-13追加。P2-3続き:
//    presentation/handlers/invoices.rs 直書きSQLのRepository層移行）。
//    手動生成（旧SSR）の関数は、未ルーティングだったため削除した ──

/// 受注書の請求書送付先メールアドレスを取得する
pub async fn find_received_order_invoice_email(pool: &::sqlx::PgPool, received_order_id: i64) -> ::anyhow::Result<Option<String>> {
    let email: Option<String> = ::sqlx::query_scalar(
        "SELECT TRIM(invoice_to_email) FROM t_received_order WHERE id = $1"
    )
    .bind(received_order_id)
    .fetch_optional(pool)
    .await?;
    Ok(email.filter(|e| !e.is_empty()))
}

/// 受注書の請求書送付先CC（カンマ区切り）を取得する
pub async fn find_received_order_invoice_cc(pool: &::sqlx::PgPool, received_order_id: i64) -> ::anyhow::Result<Option<String>> {
    let cc: Option<String> = ::sqlx::query_scalar(
        "SELECT TRIM(invoice_cc_emails) FROM t_received_order WHERE id = $1"
    )
    .bind(received_order_id)
    .fetch_optional(pool)
    .await?;
    Ok(cc.filter(|e| !e.is_empty()))
}

/// クライアントマスタの請求書送付先メールを取得する
pub async fn find_client_invoice_email(pool: &::sqlx::PgPool, client_id: i64) -> ::anyhow::Result<Option<String>> {
    let email: Option<String> = ::sqlx::query_scalar(
        "SELECT TRIM(invoice_email) FROM m_client WHERE id = $1"
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(email.filter(|e| !e.is_empty()))
}

/// クライアントマスタの請求書送付先CC（カンマ区切り）を取得する
pub async fn find_client_invoice_cc(pool: &::sqlx::PgPool, client_id: i64) -> ::anyhow::Result<Option<String>> {
    let cc: Option<String> = ::sqlx::query_scalar(
        "SELECT TRIM(cc_email) FROM m_client WHERE id = $1"
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(cc.filter(|e| !e.is_empty()))
}

/// クライアント請求書メールの宛先を解決する。
///
/// 優先順位（To）:
/// 1. 受注書の `invoice_to_email`（案件単位の請求先）
/// 2. クライアントマスタの `invoice_email`（請求連絡先・宛先）
///
/// 優先順位（Cc）:
/// 1. 受注書の `invoice_cc_emails`（案件単位）
/// 2. クライアントマスタの `cc_email`（請求書送付先 CC）
pub async fn resolve_invoice_send_recipients(
    pool: &::sqlx::PgPool,
    client_id: i64,
    received_order_id: Option<i64>,
) -> ::anyhow::Result<(Option<String>, Option<String>)> {
    if let Some(ro_id) = received_order_id {
        let to = find_received_order_invoice_email(pool, ro_id).await?;
        if let Some(to) = to {
            let cc = match find_received_order_invoice_cc(pool, ro_id).await? {
                Some(cc) => Some(cc),
                None => find_client_invoice_cc(pool, client_id).await?,
            };
            return Ok((Some(to), cc));
        }
    }
    let to = find_client_invoice_email(pool, client_id).await?;
    let cc = find_client_invoice_cc(pool, client_id).await?;
    Ok((to, cc))
}

/// 受注書番号・案件名を取得する（請求書詳細画面用）
pub async fn find_received_order_no_project_name(pool: &::sqlx::PgPool, received_order_id: i64) -> ::anyhow::Result<Option<(String, String)>> {
    let row = ::sqlx::query_as(
        "SELECT received_order_no, COALESCE(project_name, '') FROM t_received_order WHERE id = $1"
    )
    .bind(received_order_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 請求書を送信済み(SENT)にする
pub async fn mark_invoice_sent(pool: &::sqlx::PgPool, id: i64, subject: &str, body: &str) -> ::anyhow::Result<()> {
    ::sqlx::query(
        "UPDATE t_billing_invoice SET status = 'SENT', sent_at = NOW(), sent_subject = $1, sent_body = $2, updated_at = NOW() WHERE id = $3"
    )
    .bind(subject)
    .bind(body)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 請求書ヘッダーを更新する（編集画面用）
pub async fn update_invoice_header(
    pool: &::sqlx::PgPool,
    id: i64,
    issue_date: chrono::NaiveDate,
    due_date: Option<chrono::NaiveDate>,
    subject: &str,
) -> ::anyhow::Result<()> {
    // 請求書（t_billing_invoice）に備考の列は無い。以前は存在しない列 notes を書こうとして、保存が必ず失敗していた
    ::sqlx::query(
        r#"UPDATE t_billing_invoice SET
            issue_date = $1, due_date = $2,
            subject = $3, updated_at = NOW()
           WHERE id = $4"#
    )
    .bind(issue_date)
    .bind(due_date)
    .bind(subject)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 請求書を明細ごと削除する
pub async fn delete_invoice(pool: &::sqlx::PgPool, id: i64) -> ::anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    ::sqlx::query("DELETE FROM t_billing_invoice_item WHERE invoice_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    ::sqlx::query("DELETE FROM t_billing_invoice WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    crate::infrastructure::db_tx::commit_checked(tx).await?;
    Ok(())
}

/// 請求書一覧表示用の行
#[derive(Debug, Clone, ::sqlx::FromRow, ::serde::Serialize)]
pub struct InvoiceRow {
    pub id: i64,
    pub invoice_id: String,
    pub client_name: String,
    pub subject: String,
    pub project_name: Option<String>,
    pub issue_date: chrono::NaiveDate,
    pub target_month: Option<chrono::NaiveDate>,
    pub due_date: Option<chrono::NaiveDate>,
    pub total_amount: Option<i64>,
    pub item_count: i64,
    pub status: String,
    pub received_order_id: Option<i64>,
    pub client_accepted_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 請求書一覧（顧客名・案件名JOIN済み、最大100件）
pub async fn list_invoice_rows(pool: &::sqlx::PgPool) -> ::anyhow::Result<Vec<InvoiceRow>> {
    let rows = ::sqlx::query_as::<_, InvoiceRow>(
        r#"
        SELECT bi.id, bi.invoice_no AS invoice_id, c.name AS client_name, bi.subject,
               ro.project_name AS project_name,
               bi.issue_date, bi.target_month, bi.due_date,
               bi.total::BIGINT AS total_amount,
               (SELECT COUNT(*) FROM t_billing_invoice_item bii WHERE bii.invoice_id = bi.id)::BIGINT AS item_count,
               bi.status,
               bi.received_order_id,
               bi.client_accepted_at
        FROM t_billing_invoice bi
        JOIN m_client c ON c.id = bi.client_id
        LEFT JOIN t_received_order ro ON ro.id = bi.received_order_id
        ORDER BY bi.issue_date DESC LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 請求書の入金記録一覧を取得する（詳細画面用）
pub async fn list_invoice_payment_rows(pool: &::sqlx::PgPool, invoice_id: i64) -> ::anyhow::Result<Vec<(chrono::NaiveDate, i32, String, String)>> {
    let rows = ::sqlx::query_as(
        "SELECT payment_date, amount, COALESCE(method, ''), COALESCE(reference, '') FROM t_payment_record WHERE invoice_id = $1 ORDER BY payment_date"
    )
    .bind(invoice_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// クライアント向け請求書をクライアントが承諾した際の確定処理（client_accepted_at・ハッシュを更新）
pub async fn confirm_client_invoice(pool: &::sqlx::PgPool, invoice_id: i64, document_hash: &str) -> ::anyhow::Result<()> {
    ::sqlx::query(
        "UPDATE t_billing_invoice SET client_accepted_at = NOW(), document_hash = $1, updated_at = NOW() WHERE id = $2"
    )
    .bind(document_hash)
    .bind(invoice_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── WebAPI 用フィルタ・クエリ（2026-09-02追加）──

pub struct WebApiInvoiceFilter {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
    pub min_amount: Option<i32>,
    pub max_amount: Option<i32>,
}

/// WebAPI用：クライアント向け請求書一覧（フィルタ付き、最大200件）
pub async fn list_invoices_for_client(
    pool: &::sqlx::PgPool,
    client_id: i64,
    filter: &WebApiInvoiceFilter,
) -> ::anyhow::Result<Vec<BillingInvoice>> {
    let mut query = String::from(
        "SELECT * FROM t_billing_invoice WHERE client_id = $1"
    );
    let mut bind_count = 2;

    if let Some(from) = filter.from {
        query.push_str(&format!(" AND issue_date >= ${}", bind_count));
        bind_count += 1;
    }
    if let Some(to) = filter.to {
        query.push_str(&format!(" AND issue_date <= ${}", bind_count));
        bind_count += 1;
    }
    if let Some(min) = filter.min_amount {
        query.push_str(&format!(" AND total >= ${}", bind_count));
        bind_count += 1;
    }
    if let Some(max) = filter.max_amount {
        query.push_str(&format!(" AND total <= ${}", bind_count));
        bind_count += 1;
    }

    query.push_str(" ORDER BY issue_date DESC LIMIT 200");

    let mut q = ::sqlx::query_as::<_, BillingInvoice>(&query).bind(client_id);

    if let Some(from) = filter.from {
        q = q.bind(from);
    }
    if let Some(to) = filter.to {
        q = q.bind(to);
    }
    if let Some(min) = filter.min_amount {
        q = q.bind(min);
    }
    if let Some(max) = filter.max_amount {
        q = q.bind(max);
    }

    let rows = q.fetch_all(pool).await?;
    Ok(rows)
}

/// WebAPI用：特定のクライアント向け請求書を取得（IDOR防止のため client_id も検証）
pub async fn find_invoice_for_client(
    pool: &::sqlx::PgPool,
    invoice_id: i64,
    client_id: i64,
) -> ::anyhow::Result<Option<BillingInvoice>> {
    let row = ::sqlx::query_as::<_, BillingInvoice>(
        "SELECT * FROM t_billing_invoice WHERE id = $1 AND client_id = $2"
    )
    .bind(invoice_id)
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
