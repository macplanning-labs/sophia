/// infrastructure/mail_pipeline/phase2_fetch.rs — Phase2: データソース取得
///
/// 責務: 実データを取得し、`t_received_email` の行を `FETCHED`（成功）または
///       `FETCH_FAILED`（失敗・リトライ対象）に更新するだけ。パース・DB登録はPhase3の責務。
///
/// 2種類の取得経路を独立に実行する:
///
/// 1. EDI-OASIS APIポーリング（`m_client.edi_system_type = 'EDI_OASIS'` のクライアント対象）
///    メール個々の行には依存しない（メールが1通も届いていなくても実行される）。
///    当月+翌月を `fetch_orders`/`fetch_invoices` でポーリングし、`t_received_order` に
///    未登録の注文・請求だけを対象に詳細JSON + PDFを取得する。取得したJSONは既に構造化済み
///    のため、合成した `message_id`（例: `edi-order:{client_id}:{oasis_id}`）で新規行を作り、
///    最初から `status='FETCHED'` として保存する（Phase3は追加パース不要でそのまま登録する）。
///    重複実行の冪等性は message_id の UNIQUE 制約でそのまま担保される。
///
/// 2. 添付ファイル系メール（`status='NEW' AND source_type='ATTACHMENT'`）
///    Phase1が見つけた行ごとに、IMAPで該当メールを再取得（EXAMINEで再フェッチ、既読状態は
///    変更しない）し、添付バイナリを実体化して `raw_attachment` 列に保存する。
///
/// ## 認証エラー(401)の扱い
/// `EdiOasisClient` が `EdiOasisError::Unauthorized` を返した場合、1回だけ `login()` を
/// 呼び直して同じ操作をリトライする（`with_retry!` マクロ）。再ログインも失敗した場合は
/// そのクライアントの処理をスキップし、次回スケジューラ実行時に再試行される。

use chrono::{Datelike, Utc};
use sqlx::PgPool;

use super::imap_util::{fetch_attachments_blocking, Attachment, ImapConfig};
use super::ops_message;
use crate::domain::models::mail_pipeline::MAX_RETRY;
use crate::infrastructure::edi_oasis_client::{EdiOasisClient, EdiOasisError, InvoiceSummary, OrderSummary};

/// 401(Unauthorized)なら1回だけ再ログインしてリトライするマクロ。
/// `$client` は `&mut EdiOasisClient`、`$call` は再評価可能な式（例: `$client.fetch_orders(y, m)`）。
macro_rules! with_retry {
    ($client:expr, $call:expr) => {{
        match $call.await {
            Ok(v) => Ok(v),
            Err(e) => {
                if matches!(e, EdiOasisError::Unauthorized(_)) {
                    tracing::warn!("[Phase2] OASISセッション切れを検知、再ログインして再試行します");
                    match $client.login().await {
                        Ok(()) => $call.await,
                        Err(login_err) => {
                            ops_message::log_error(ops_message::fail(
                                "メール取込",
                                "Phase2",
                                "EDI-OASIS再ログイン",
                                "当該クライアントのEDI取得をスキップ（次回再試行）",
                                &login_err,
                            ));
                            Err(e)
                        }
                    }
                } else {
                    Err(e)
                }
            }
        }
    }};
}

/// Phase2実行結果
#[derive(Debug, Default)]
pub struct Phase2Result {
    pub edi_polled_clients: usize,
    pub edi_new_orders: usize,
    pub edi_new_invoices: usize,
    pub attachment_fetched: usize,
    pub attachment_failed: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for Phase2Result {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EDIクライアント{}件走査(新規注文{}件/新規請求{}件), 添付取得{}件成功/{}件失敗, エラー{}件",
            self.edi_polled_clients, self.edi_new_orders, self.edi_new_invoices,
            self.attachment_fetched, self.attachment_failed, self.errors.len()
        )
    }
}

/// Phase2エントリポイント
///
/// `override_month`: `Some((year, month))` の場合、EDI-OASIS APIポーリング対象をその月「のみ」に
/// 限定する（ダッシュボードで年月を指定した手動一括取込用）。`None` の場合は従来通り
/// 「当月+翌月」を自動ポーリングする（スケジューラ・通常の手動トリガー用）。
pub async fn run(pool: &PgPool, override_month: Option<(i32, i32)>) -> Phase2Result {
    let mut result = Phase2Result::default();

    fetch_edi_api_sources(pool, override_month, &mut result).await;
    fetch_attachment_sources(pool, &mut result).await;

    tracing::info!("[Phase2] 完了: {result}");
    result
}

// ============================================================
// 1. EDI-OASIS APIポーリング
// ============================================================

async fn fetch_edi_api_sources(pool: &PgPool, override_month: Option<(i32, i32)>, result: &mut Phase2Result) {
    #[derive(sqlx::FromRow)]
    struct EdiClient {
        id: i64,
        name: String,
        email: String,
    }

    let clients: Vec<EdiClient> = match sqlx::query_as(
        "SELECT id, name, email FROM m_client WHERE edi_system_type = 'EDI_OASIS'",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "EDIクライアント一覧取得",
                "当該クライアントのEDI取得をスキップ（次回再試行）",
                format!("{}", ops_message::format_sqlx(&e)),
            ));
            return;
        }
    };

    if clients.is_empty() {
        tracing::debug!("[Phase2] EDI_OASIS設定済みクライアントなし");
        return;
    }

    let mut client = match EdiOasisClient::from_env() {
        Ok(c) => c,
        Err(EdiOasisError::NotConfigured) => {
            tracing::info!("[Phase2] EDI-OASIS認証情報未設定、APIポーリングをスキップ");
            return;
        }
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "EDI-OASIS初期化",
                "当該クライアントのEDI取得をスキップ（次回再試行）",
                &e,
            ));
            return;
        }
    };

    if let Err(e) = client.login().await {
        result.errors.push(ops_message::fail(
            "メール取込",
            "Phase2",
            "EDI-OASISログイン",
            "当該クライアントのEDI取得をスキップ（次回再試行）",
            &e,
        ));
        return;
    }

    let target_months: Vec<(i32, i32)> = match override_month {
        Some((y, m)) => vec![(y, m)],
        None => {
            let today = Utc::now().date_naive();
            let (next_year, next_month) = if today.month() == 12 {
                (today.year() + 1, 1)
            } else {
                (today.year(), today.month() as i32 + 1)
            };
            vec![(today.year(), today.month() as i32), (next_year, next_month)]
        }
    };

    for c in &clients {
        result.edi_polled_clients += 1;

        for &(year, month) in &target_months {
            poll_orders(pool, &mut client, c.id, &c.name, &c.email, year, month, result).await;
            poll_invoices(pool, &mut client, c.id, &c.name, &c.email, year, month, result).await;
        }
    }
}

async fn poll_orders(
    pool: &PgPool,
    client: &mut EdiOasisClient,
    client_id: i64,
    client_name: &str,
    client_email: &str,
    year: i32,
    month: i32,
    result: &mut Phase2Result,
) {
    let orders: Vec<OrderSummary> = match with_retry!(client, client.fetch_orders(year, month)) {
        Ok(o) => o,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "EDI注文一覧取得",
                "当該クライアントの注文取得をスキップ（次回再試行）",
                format!("client={client_name}, {year}/{month}: {e}"),
            ));
            return;
        }
    };

    for order in &orders {
        if order.is_received || order.order_no.is_empty() {
            continue;
        }
        match register_edi_order(pool, client, client_id, client_name, client_email, order).await {
            Ok(true) => result.edi_new_orders += 1,
            Ok(false) => {}
            Err(e) => result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "EDI注文詳細取得",
                "当該注文の取得をスキップ（次回再試行）",
                format!("client={client_name}, order_no={}: {e}", order.order_no),
            )),
        }
    }
}

async fn poll_invoices(
    pool: &PgPool,
    client: &mut EdiOasisClient,
    client_id: i64,
    client_name: &str,
    client_email: &str,
    year: i32,
    month: i32,
    result: &mut Phase2Result,
) {
    let invoices: Vec<InvoiceSummary> = match with_retry!(client, client.fetch_invoices(year, month)) {
        Ok(i) => i,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "EDI請求一覧取得",
                "当該クライアントの請求取得をスキップ（次回再試行）",
                format!("client={client_name}, {year}/{month}: {e}"),
            ));
            return;
        }
    };

    for invoice in &invoices {
        if invoice.invoice_no.is_empty() {
            continue;
        }
        match register_edi_invoice(pool, client, client_id, client_name, client_email, invoice).await {
            Ok(true) => result.edi_new_invoices += 1,
            Ok(false) => {}
            Err(e) => result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "EDI請求詳細取得",
                "当該請求書の取得をスキップ（次回再試行）",
                format!("client={client_name}, invoice_no={}: {e}", invoice.invoice_no),
            )),
        }
    }
}

/// 未取込の注文を1件取得し、詳細JSON+PDFを添えて t_received_email に FETCHED として保存する
async fn register_edi_order(
    pool: &PgPool,
    client: &mut EdiOasisClient,
    client_id: i64,
    client_name: &str,
    client_email: &str,
    order: &OrderSummary,
) -> Result<bool, String> {
    // 既に受注登録済みか（client_id + client_order_numberで判定）
    let already_registered: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_received_order WHERE client_id = $1 AND client_order_number = $2)",
    )
    .bind(client_id)
    .bind(&order.order_no)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("重複判定(DB読取)失敗(既存受注): {e}"))?;
    if already_registered {
        return Ok(false);
    }

    let message_id = format!("edi-order:{client_id}:{}", order.id);
    let already_fetched: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM t_received_email WHERE message_id = $1)")
            .bind(&message_id)
            .fetch_one(pool)
            .await
            .map_err(|e| format!("重複判定(DB読取)失敗(既取込判定): {e}"))?;
    if already_fetched {
        return Ok(false);
    }

    let detail = with_retry!(client, client.fetch_order_detail(order.id))
        .map_err(|e| format!("注文詳細取得失敗: {e}"))?;
    let pdf_bytes = client.download_order_pdf(order.id).await.ok().flatten();
    let parsed_data = serde_json::to_value(&detail).unwrap_or(serde_json::Value::Null);

    let rows_affected = sqlx::query(
        r#"
        INSERT INTO t_received_email (
            message_id, from_email, from_name, subject, received_at,
            client_id, status, source_type, attachment_filename, parsed_data, raw_attachment
        ) VALUES ($1, $2, $3, $4, NOW(), $5, 'FETCHED', 'EDI_API', $6, $7, $8)
        ON CONFLICT (message_id) DO NOTHING
        "#,
    )
    .bind(&message_id)
    .bind(client_email)
    .bind(client_name)
    .bind(format!("EDI-OASIS注文 {}", order.order_no))
    .bind(client_id)
    .bind(format!("注文書_{}_{}.pdf", client_name, order.order_no))
    .bind(&parsed_data)
    .bind(pdf_bytes.as_deref())
    .execute(pool)
    .await
    .map_err(|e| format!("DB保存エラー: {e}"))?
    .rows_affected();

    Ok(rows_affected > 0)
}

/// 未取込の請求書を1件取得し、詳細JSON+PDFを添えて t_received_email に FETCHED として保存する
async fn register_edi_invoice(
    pool: &PgPool,
    client: &mut EdiOasisClient,
    client_id: i64,
    client_name: &str,
    client_email: &str,
    invoice: &InvoiceSummary,
) -> Result<bool, String> {
    // 既に対象月の請求書とリンク済みか（client_id + edi_invoice_numberで判定）
    let already_linked: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_billing_invoice WHERE client_id = $1 AND edi_invoice_no = $2)",
    )
    .bind(client_id)
    .bind(&invoice.invoice_no)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("重複判定(DB読取)失敗(既存請求): {e}"))?;
    if already_linked {
        return Ok(false);
    }

    let message_id = format!("edi-invoice:{client_id}:{}", invoice.id);
    let already_fetched: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM t_received_email WHERE message_id = $1)")
            .bind(&message_id)
            .fetch_one(pool)
            .await
            .map_err(|e| format!("重複判定(DB読取)失敗(既取込判定): {e}"))?;
    if already_fetched {
        return Ok(false);
    }

    let detail = with_retry!(client, client.fetch_invoice_detail(invoice.id))
        .map_err(|e| format!("請求書詳細取得失敗: {e}"))?;
    // 請求書PDF・支払通知書PDFのうち、金額確定情報として優先度の高い方を保存する
    let (invoice_pdf, payment_notice_pdf) = client
        .download_invoice_pdfs(invoice.id)
        .await
        .unwrap_or((None, None));
    let pdf_bytes = payment_notice_pdf.or(invoice_pdf);
    let parsed_data = serde_json::to_value(&detail).unwrap_or(serde_json::Value::Null);

    let rows_affected = sqlx::query(
        r#"
        INSERT INTO t_received_email (
            message_id, from_email, from_name, subject, received_at,
            client_id, status, source_type, attachment_filename, parsed_data, raw_attachment
        ) VALUES ($1, $2, $3, $4, NOW(), $5, 'FETCHED', 'EDI_API', $6, $7, $8)
        ON CONFLICT (message_id) DO NOTHING
        "#,
    )
    .bind(&message_id)
    .bind(client_email)
    .bind(client_name)
    .bind(format!("EDI-OASIS請求書 {}", invoice.invoice_no))
    .bind(client_id)
    .bind(format!("請求書_{}_{}.pdf", client_name, invoice.invoice_no))
    .bind(&parsed_data)
    .bind(pdf_bytes.as_deref())
    .execute(pool)
    .await
    .map_err(|e| format!("DB保存エラー: {e}"))?
    .rows_affected();

    Ok(rows_affected > 0)
}

// ============================================================
// 2. 添付ファイル系メールの再取得
// ============================================================

async fn fetch_attachment_sources(pool: &PgPool, result: &mut Phase2Result) {
    #[derive(sqlx::FromRow)]
    struct PendingEmail {
        id: i64,
        message_id: String,
        retry_count: i32,
    }

    let pending: Vec<PendingEmail> = match sqlx::query_as(
        r#"
        SELECT id, message_id, retry_count FROM t_received_email
        WHERE source_type = 'ATTACHMENT'
          AND status IN ('NEW', 'FETCH_FAILED')
          AND (next_retry_at IS NULL OR next_retry_at <= NOW())
        ORDER BY received_at
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "添付対象メール一覧のDB読取",
                "添付対象メールが見つからず添付再取得をスキップ",
                format!("{}", ops_message::format_sqlx(&e)),
            ));
            return;
        }
    };

    if pending.is_empty() {
        return;
    }

    let config = match ImapConfig::load(pool).await {
        Ok(c) => c,
        Err(e) => {
            let kind = ops_message::classify_phase1_fatal(&e);
            let process = match kind {
                ops_message::Phase1FatalKind::Database => "DB読取(自社情報のIMAP認証)",
                ops_message::Phase1FatalKind::ConfigMissing => "IMAP認証情報未設定",
                _ => "IMAP設定読込"
            };
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                process,
                "添付のIMAP取得をスキップ（次回再試行）",
                &e,
            ));
            return;
        }
    };

    // IMAP通信(接続・検索・取得)は同期処理のため、Tokioのワーカースレッドをブロックしないよう
    // spawn_blocking上でまとめて実行する（#073再発防止、開発標準書§2.3）。
    let message_ids: Vec<String> = pending.iter().map(|p| p.message_id.clone()).collect();
    let fetch_results = match tokio::task::spawn_blocking(move || {
        fetch_attachments_blocking(&config, &message_ids)
    })
    .await
    {
        Ok(Ok(results)) => results,
        Ok(Err(e)) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "添付のIMAP取得",
                "当該メール添付の取得に失敗（次回再試行）",
                &e,
            ));
            return;
        }
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase2",
                "添付取得タスク実行",
                "IMAP処理の実行に失敗（次回再試行）",
                e,
            ));
            return;
        }
    };

    for (email, fetch_result) in pending.iter().zip(fetch_results.into_iter()) {
        let save_result = match fetch_result {
            Ok(attachment) => save_attachment(pool, email.id, &attachment).await,
            Err(e) => Err(e),
        };

        match save_result {
            Ok(()) => result.attachment_fetched += 1,
            Err(e) => {
                result.attachment_failed += 1;
                result.errors.push(ops_message::fail(
                    "メール取込",
                    "Phase2",
                    "添付バイナリのDB保存",
                    "当該メール添付の保存に失敗",
                    format!("メールID={} | {}", email.id, e),
                ));
                mark_fetch_failed(pool, email.id, email.retry_count, &e).await;
            }
        }
    }
}

/// 添付バイナリをDBへ保存する（IMAP取得は `fetch_attachments_blocking`（spawn_blocking経由）で完了済み）
async fn save_attachment(pool: &PgPool, email_id: i64, attachment: &Attachment) -> Result<(), String> {
    sqlx::query(
        r#"
        UPDATE t_received_email
        SET status = 'FETCHED',
            attachment_filename = $2,
            raw_attachment = $3,
            error_message = ''
        WHERE id = $1
        "#,
    )
    .bind(email_id)
    .bind(&attachment.filename)
    .bind(&attachment.bytes)
    .execute(pool)
    .await
    .map_err(|e| format!("DB更新エラー: {e}"))?;

    tracing::info!(
        "[Phase2] 添付取得成功: メールID={email_id}, filename={}, {}bytes",
        attachment.filename,
        attachment.bytes.len()
    );

    Ok(())
}

/// 取得失敗をFETCH_FAILEDとして記録し、リトライ回数に応じてnext_retry_at/needs_manual_reviewを更新する
async fn mark_fetch_failed(pool: &PgPool, email_id: i64, current_retry_count: i32, error: &str) {
    let new_retry_count = current_retry_count + 1;
    let needs_review = new_retry_count >= MAX_RETRY;
    // 指数バックオフ（次回リトライまで 2^retry 時間、最大24時間）
    let backoff_hours = (1i64 << new_retry_count.min(4)).min(24i64);

    let result = sqlx::query(
        r#"
        UPDATE t_received_email
        SET status = 'FETCH_FAILED',
            retry_count = $2,
            next_retry_at = NOW() + ($3 || ' hours')::INTERVAL,
            needs_manual_review = $4,
            error_message = $5
        WHERE id = $1
        "#,
    )
    .bind(email_id)
    .bind(new_retry_count)
    .bind(backoff_hours.to_string())
    .bind(needs_review)
    .bind(format!("[Phase2] {error}"))
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!("[Phase2] FETCH_FAILED更新エラー (メールID={email_id}): {e}");
    }
}
