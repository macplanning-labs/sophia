/// infrastructure/mail_pipeline/phase3_parse_register.rs — Phase3: データ解析・システム登録
///
/// 責務: `status='FETCHED'` の行を対象に、書類種別ごとに解析して社内DBへ登録し、
///       `status` を `IMPORTED`（成功）/ `SKIPPED`（重複=正常）/ `PARSE_FAILED`（失敗）へ更新する。
///
/// ## 振り分けロジック
/// - `source_type='EDI_API'`（message_idが`edi-order:`/`edi-invoice:`で始まる合成行）:
///   Phase2が既に構造化JSONを`parsed_data`へ格納済みのため、追加パース不要でそのままDB登録する。
/// - `source_type='ATTACHMENT'`: `attachment_filename`の拡張子と件名分類(注文/請求/稼働報告)から
///   PDF注文書・PDF支払通知書/請求書・Excel稼働報告書のいずれかを判定し、対応するパーサーに回す。
///
/// ## エラー分類
/// `PhaseError::Permanent` は即座に `needs_manual_review=TRUE`（フォーマット不正・必須マスタ未登録等）。
/// `PhaseError::Transient` は `retry_count` を進めて次回再試行（DB接続断等、稀）。
/// 重複（既に取込済み）はエラーではなく `SKIPPED` として扱う。

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use sqlx::PgPool;

use super::ops_message;
use crate::domain::models::mail_pipeline::{PhaseError, MAX_RETRY};
use crate::infrastructure::attachment_parsers::{self, DocumentKind, ParsedDocument};
use crate::infrastructure::edi_oasis_client::{InvoiceDetail, OrderDetail};
use crate::infrastructure::edi_order_importer;
use crate::infrastructure::repositories::order_repo;

/// Phase3実行結果
#[derive(Debug, Default)]
pub struct Phase3Result {
    pub imported: usize,
    pub skipped_duplicate: usize,
    pub parse_failed: usize,
    pub errors: Vec<String>,
    /// 今回新規登録された注文書（重複スキップは含まない）。呼び出し側で「何が新規追加されたか」を通知するのに使う
    pub new_orders: Vec<String>,
    /// 今回新規に突合できた支払通知書/請求書
    pub new_invoices: Vec<String>,
    /// 今回新規登録された稼働報告
    pub new_timesheets: Vec<String>,
}

impl std::fmt::Display for Phase3Result {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "取込{}件, 重複スキップ{}件, パース失敗{}件, エラー{}件",
            self.imported, self.skipped_duplicate, self.parse_failed, self.errors.len()
        )
    }
}

/// 登録結果（重複は失敗ではなく正常なスキップとして区別する）
enum RegisterOutcome {
    Registered(RegisteredItem),
    AlreadyExists,
}

/// 新規登録された書類の種別+人が読める識別ラベル
enum RegisteredItem {
    Order(String),
    Invoice(String),
    Timesheet(String),
}

#[derive(Debug, sqlx::FromRow)]
struct FetchedEmail {
    id: i64,
    message_id: String,
    subject: String,
    source_type: String,
    client_id: Option<i64>,
    attachment_filename: String,
    raw_attachment: Option<Vec<u8>>,
    parsed_data: Option<serde_json::Value>,
    retry_count: i32,
}

/// Phase3エントリポイント
pub async fn run(pool: &PgPool) -> Phase3Result {
    let mut result = Phase3Result::default();

    // FETCHED / PARSE_FAILED とも needs_manual_review=FALSE のみ。
    // 同月既存で候補保留した行（FETCHED + needs_manual_review=TRUE）や Permanent 確定行は
    // 人間対応待ちのため再処理しない（毎バッチ AlreadyExists ループを防ぐ）。
    let rows: Vec<FetchedEmail> = match sqlx::query_as(
        r#"
        SELECT id, message_id, subject, source_type, client_id,
               attachment_filename, raw_attachment, parsed_data, retry_count
        FROM t_received_email
        WHERE status IN ('FETCHED', 'PARSE_FAILED')
          AND needs_manual_review = FALSE
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
                "Phase3",
                "対象メール一覧のDB読取",
                "業務テーブル登録をスキップ（次回再試行）",
                format!("{}", ops_message::format_sqlx(&e)),
            ));
            return result;
        }
    };

    for row in &rows {
        match process_one(pool, row).await {
            Ok(RegisterOutcome::Registered(item)) => {
                mark_status(pool, row.id, "IMPORTED", false, None, row.retry_count).await;
                result.imported += 1;
                match item {
                    RegisteredItem::Order(label) => result.new_orders.push(label),
                    RegisteredItem::Invoice(label) => result.new_invoices.push(label),
                    RegisteredItem::Timesheet(label) => result.new_timesheets.push(label),
                }
            }
            Ok(RegisterOutcome::AlreadyExists) => {
                // 同月に既に登録がある場合も、メール行は候補として残す（needs_manual_review=TRUE）
                // 本登録はしないが、ダッシュボード・カードには選択肢として表示する
                mark_status(pool, row.id, "FETCHED", true, Some("同月既存のため本登録スキップ。候補として保留中"), row.retry_count).await;
                result.skipped_duplicate += 1;
            }
            Err(err) => {
                let needs_review = err.is_permanent() || row.retry_count + 1 >= MAX_RETRY;
                let msg = crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &err);
                mark_status(pool, row.id, "PARSE_FAILED", needs_review, Some(&msg), row.retry_count).await;
                result.parse_failed += 1;
                // Permanent(想定内の業務例外・要確認)はerrorsに含めない。
                // needs_manual_review + Phase4のADMINアラートで十分に可視化されるため、
                // 「パイプラインが技術的に失敗した」ことを示すerrorsに混ぜるとhas_errors()が
                // 誤って発火し、ダッシュボードの「一括取込」ボタンが正常時にもエラー扱いになってしまう。
                if !err.is_permanent() {
                    result.errors.push(ops_message::fail(
                        "メール取込",
                        "Phase3",
                        "業務テーブル登録",
                        "取込スキップ（再試行対象またはリトライ上限）",
                        format!("メールID={} | {msg}", row.id),
                    ));
                }
            }
        }
    }

    tracing::info!("[Phase3] 完了: {result}");
    result
}

async fn mark_status(
    pool: &PgPool,
    email_id: i64,
    status: &str,
    needs_manual_review: bool,
    error_message: Option<&str>,
    current_retry_count: i32,
) {
    let retry_count = if status == "PARSE_FAILED" { current_retry_count + 1 } else { current_retry_count };
    // 指数バックオフ（次回リトライまで 2^retry 時間、最大24時間）。Phase2のFETCH_FAILEDと同じ方式。
    let backoff_hours: i64 = (1i64 << retry_count.min(4)).min(24i64);

    let result = sqlx::query(
        r#"
        UPDATE t_received_email
        SET status = $2,
            needs_manual_review = needs_manual_review OR $3,
            error_message = COALESCE($4, error_message),
            retry_count = $5,
            next_retry_at = CASE WHEN $2 = 'PARSE_FAILED' THEN NOW() + ($6 || ' hours')::INTERVAL ELSE next_retry_at END,
            processed_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(email_id)
    .bind(status)
    .bind(needs_manual_review)
    .bind(error_message)
    .bind(retry_count)
    .bind(backoff_hours.to_string())
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::error!("[Phase3] ステータス更新エラー (メールID={email_id}): {e}");
    }
}

/// 1件を解析・登録する
async fn process_one(pool: &PgPool, row: &FetchedEmail) -> Result<RegisterOutcome, PhaseError> {
    if row.source_type == "EDI_API" {
        if row.message_id.starts_with("edi-order:") {
            return register_edi_order(pool, row).await;
        } else if row.message_id.starts_with("edi-invoice:") {
            return register_edi_invoice(pool, row).await;
        }
        return Err(PhaseError::Permanent(format!(
            "未知のEDI_API合成message_id形式: {}",
            row.message_id
        )));
    }

    // ATTACHMENT: 件名分類 + 拡張子で振り分け
    let label = super::phase1_watch::classify_subject_label(&row.subject);
    let filename_lower = row.attachment_filename.to_lowercase();
    let raw_bytes = row
        .raw_attachment
        .as_ref()
        .ok_or_else(|| PhaseError::Permanent("添付ファイルの実体がありません".to_string()))?;

    if filename_lower.ends_with(".pdf") {
        match label.as_str() {
            "ORDER" => register_order_pdf(pool, row, raw_bytes).await,
            "INVOICE" => register_payment_notice_pdf(pool, row, raw_bytes).await,
            "REPORT" => register_timesheet_attachment(pool, row, raw_bytes).await,
            _ => Err(PhaseError::Permanent(format!(
                "件名からPDF種別を判定できません（分類={label}）"
            ))),
        }
    } else if filename_lower.ends_with(".xlsx")
        || filename_lower.ends_with(".xlsm")
        || filename_lower.ends_with(".xls")
    {
        register_timesheet_attachment(pool, row, raw_bytes).await
    } else {
        Err(PhaseError::Permanent(format!(
            "未対応の添付ファイル形式です: {}",
            row.attachment_filename
        )))
    }
}

// ============================================================
// EDI_API（Phase2が構造化JSONを取得済み） — 注文
// ============================================================

async fn register_edi_order(pool: &PgPool, row: &FetchedEmail) -> Result<RegisterOutcome, PhaseError> {
    let client_id = row
        .client_id
        .ok_or_else(|| PhaseError::Permanent("client_idが設定されていません".to_string()))?;
    let data = row
        .parsed_data
        .clone()
        .ok_or_else(|| PhaseError::Permanent("parsed_dataが空です".to_string()))?;
    let detail: OrderDetail = serde_json::from_value(data)
        .map_err(|e| PhaseError::Permanent(format!("OrderDetail JSONパースエラー: {e}")))?;

    let order_no = detail.order_no.clone().unwrap_or_default();
    if order_no.is_empty() {
        return Err(PhaseError::Permanent("注文番号が取得できません".to_string()));
    }

    let (target_month, work_start, work_end) = edi_order_importer::parse_dates(&detail)
        .map_err(PhaseError::Permanent)?;

    let normalized = NormalizedOrder {
        client_id,
        client_order_number: order_no,
        order_date: detail
            .order_date
            .as_deref()
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .unwrap_or(target_month),
        target_month,
        project_name: detail.project_name.clone().unwrap_or_else(|| "EDI注文".to_string()),
        work_start,
        work_end,
        unit_price: detail.unit_price.unwrap_or(0) as i32,
        worker_name: detail.worker_name.clone(),
        // EDI-OASIS APIのJSONには基準時間/超過控除単価が含まれないため、既存運用の既定値を踏襲する
        lower_limit_hours: Decimal::new(1400, 1),
        upper_limit_hours: Decimal::new(1800, 1),
        excess_rate: detail.unit_price.unwrap_or(0) as i32,
        shortage_rate: detail.unit_price.unwrap_or(0) as i32,
        remarks: format!("EDI-OASIS自動取込 (message_id={})", row.message_id),
    };

    insert_received_order(pool, &normalized).await
}

// ============================================================
// ATTACHMENT PDF — 注文書
// ============================================================

async fn register_order_pdf(
    pool: &PgPool,
    row: &FetchedEmail,
    raw_bytes: &[u8],
) -> Result<RegisterOutcome, PhaseError> {
    let client_id = row
        .client_id
        .ok_or_else(|| PhaseError::Permanent("注文書の送信元クライアントを特定できません".to_string()))?;

    let parsed = match attachment_parsers::parse_pdf(DocumentKind::Order, raw_bytes)? {
        ParsedDocument::Order(o) => o,
        _ => unreachable!(),
    };

    let order_no = parsed
        .client_order_number
        .clone()
        .ok_or_else(|| PhaseError::Permanent("PDFから注文番号を抽出できませんでした".to_string()))?;
    let unit_price = parsed
        .unit_price
        .ok_or_else(|| PhaseError::Permanent("PDFから単価を抽出できませんでした".to_string()))?
        as i32;
    let work_start = parsed
        .work_start
        .ok_or_else(|| PhaseError::Permanent("PDFから作業開始日を抽出できませんでした".to_string()))?;
    let work_end = parsed.work_end.unwrap_or(work_start);
    let target_month = NaiveDate::from_ymd_opt(work_start.year(), work_start.month(), 1)
        .unwrap_or(work_start);

    let normalized = NormalizedOrder {
        client_id,
        client_order_number: order_no,
        order_date: parsed.order_date.unwrap_or(work_start),
        target_month,
        project_name: parsed.project_name.unwrap_or_else(|| "PDF注文書".to_string()),
        work_start,
        work_end,
        unit_price,
        worker_name: parsed.person_name.clone(),
        lower_limit_hours: parsed
            .time_lower
            .and_then(Decimal::from_f64_retain)
            .unwrap_or(Decimal::new(1400, 1)),
        upper_limit_hours: parsed
            .time_upper
            .and_then(Decimal::from_f64_retain)
            .unwrap_or(Decimal::new(1800, 1)),
        excess_rate: parsed.excess_rate.map(|v| v as i32).unwrap_or(unit_price),
        shortage_rate: parsed.shortage_rate.map(|v| v as i32).unwrap_or(unit_price),
        remarks: format!("添付PDF自動取込 ({}, フォーマット={})", row.attachment_filename, parsed.format),
    };

    insert_received_order(pool, &normalized).await
}

// ── 共通: 正規化済み注文データのDB登録 ──

struct NormalizedOrder {
    client_id: i64,
    client_order_number: String,
    order_date: NaiveDate,
    target_month: NaiveDate,
    project_name: String,
    work_start: NaiveDate,
    work_end: NaiveDate,
    unit_price: i32,
    worker_name: Option<String>,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    excess_rate: i32,
    shortage_rate: i32,
    remarks: String,
}

async fn insert_received_order(
    pool: &PgPool,
    o: &NormalizedOrder,
) -> Result<RegisterOutcome, PhaseError> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_received_order WHERE client_id = $1 AND client_order_number = $2)",
    )
    .bind(o.client_id)
    .bind(&o.client_order_number)
    .fetch_one(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("重複チェックエラー: {e}")))?;

    if exists {
        return Ok(RegisterOutcome::AlreadyExists);
    }

    let engineer_id: Option<i64> = match &o.worker_name {
        Some(worker) => {
            match crate::infrastructure::billing_importer::find_or_create_engineer(pool, worker).await {
                Ok(id) => Some(id),
                Err(e) => {
                    tracing::warn!("[Phase3] エンジニア解決失敗: '{worker}' → {e}");
                    None
                }
            }
        }
        None => None,
    };

    let order_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            client_id, engineer_id, target_month, work_start, work_end,
            project_name, status, order_date, client_order_number, remarks
        ) VALUES ($1, $2, $3, $4, $5, $6, 'REGISTERED', $7, $8, $9)
        RETURNING id
        "#,
    )
    .bind(o.client_id)
    .bind(engineer_id)
    .bind(o.target_month)
    .bind(o.work_start)
    .bind(o.work_end)
    .bind(&o.project_name)
    .bind(o.order_date)
    .bind(&o.client_order_number)
    .bind(&o.remarks)
    .fetch_one(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("受注INSERTエラー: {e}")))?;

    if let Some(ref worker) = o.worker_name {
        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO t_received_order_item (
                order_id, engineer_name, unit_price, man_month,
                settlement_type, base_rate, lower_limit_hours, upper_limit_hours,
                deduction_rate, overtime_rate, effort, mid_month_rule, amount
            ) VALUES ($1, $2, $3, 1.0, 'RANGE', $3, $4, $5, $6, $7, 1.0, 'FULL', $3)
            "#,
        )
        .bind(order_id)
        .bind(worker)
        .bind(o.unit_price)
        .bind(o.lower_limit_hours)
        .bind(o.upper_limit_hours)
        .bind(o.shortage_rate)
        .bind(o.excess_rate)
        .execute(pool)
        .await
        {
            tracing::error!("[Phase3] 受注明細INSERTエラー(order_id={order_id}): {e}");
        }
    }

    match order_repo::try_auto_link_received_order_contract(pool, order_id).await {
        Ok(outcome) => order_repo::log_auto_link_outcome("Phase3", order_id, &outcome),
        Err(e) => tracing::warn!("[Phase3] 受注契約自動紐付けエラー(order_id={order_id}): {e}"),
    }

    tracing::info!(
        "[Phase3] 受注登録完了: order_id={order_id}, client_id={}, order_no={}",
        o.client_id, o.client_order_number
    );

    let label = format!(
        "{}（{}, {}）",
        o.client_order_number,
        o.target_month.format("%Y年%m月"),
        o.project_name
    );
    Ok(RegisterOutcome::Registered(RegisteredItem::Order(label)))
}

// ============================================================
// EDI_API（構造化JSON） — 請求書 / ATTACHMENT PDF — 支払通知書・請求書
// ============================================================

async fn register_edi_invoice(pool: &PgPool, row: &FetchedEmail) -> Result<RegisterOutcome, PhaseError> {
    let client_id = row
        .client_id
        .ok_or_else(|| PhaseError::Permanent("client_idが設定されていません".to_string()))?;
    let data = row
        .parsed_data
        .clone()
        .ok_or_else(|| PhaseError::Permanent("parsed_dataが空です".to_string()))?;
    let detail: InvoiceDetail = serde_json::from_value(data)
        .map_err(|e| PhaseError::Permanent(format!("InvoiceDetail JSONパースエラー: {e}")))?;

    let (year, month) = match (detail.year, detail.month) {
        (Some(y), Some(m)) => (y, m as u32),
        _ => return Err(PhaseError::Permanent("請求書の対象年月が取得できません".to_string())),
    };
    let target_month = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| PhaseError::Permanent(format!("無効な年月: {year}/{month}")))?;

    let total: i64 = detail.items.iter().map(|i| i.item_amount as i64).sum();
    let invoice_no = detail.invoice_no.clone().unwrap_or_default();

    reconcile_billing_invoice(pool, client_id, target_month, &invoice_no, total).await
}

async fn register_payment_notice_pdf(
    pool: &PgPool,
    row: &FetchedEmail,
    raw_bytes: &[u8],
) -> Result<RegisterOutcome, PhaseError> {
    let client_id = row
        .client_id
        .ok_or_else(|| PhaseError::Permanent("支払通知書/請求書の送信元クライアントを特定できません".to_string()))?;

    let parsed = match attachment_parsers::parse_pdf(DocumentKind::PaymentNotice, raw_bytes)? {
        ParsedDocument::PaymentNotice(p) => p,
        _ => unreachable!(),
    };

    let target_month = parsed
        .target_month
        .ok_or_else(|| PhaseError::Permanent("PDFから対象月を抽出できませんでした".to_string()))?;
    let total = parsed
        .total
        .ok_or_else(|| PhaseError::Permanent("PDFから合計金額を抽出できませんでした".to_string()))?;
    let invoice_no = parsed.invoice_number.clone().unwrap_or_default();

    reconcile_billing_invoice(pool, client_id, target_month, &invoice_no, total).await
}

/// EDI/PDFから取得した金額を、既存の自社発行請求書(t_billing_invoice)と突合してリンクする。
///
/// 対象月+クライアントで一致する請求書がなければ Permanent エラー（要確認）とする。
/// 誤った金額で新規の会計レコードを自動生成するリスクを避けるため、あくまで既存行への
/// リンク・確認のみを行う（新規INSERTはしない）。
async fn reconcile_billing_invoice(
    pool: &PgPool,
    client_id: i64,
    target_month: NaiveDate,
    edi_invoice_no: &str,
    edi_total: i64,
) -> Result<RegisterOutcome, PhaseError> {
    #[derive(sqlx::FromRow)]
    struct ExistingInvoice {
        id: i64,
        total: i32,
        edi_invoice_no: String,
    }

    let existing: Option<ExistingInvoice> = sqlx::query_as(
        "SELECT id, total, edi_invoice_no FROM t_billing_invoice WHERE client_id = $1 AND target_month = $2",
    )
    .bind(client_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("請求書検索エラー: {e}")))?;

    let Some(existing) = existing else {
        return Err(PhaseError::Permanent(format!(
            "対象月({target_month})の請求書が見つかりません（client_id={client_id}）。先方の金額(¥{edi_total})との突合には手動確認が必要です"
        )));
    };

    if !existing.edi_invoice_no.is_empty() && existing.edi_invoice_no != edi_invoice_no {
        return Err(PhaseError::Permanent(format!(
            "請求書ID={}は既に別のEDI請求番号({})とリンク済みです（今回: {edi_invoice_no}）",
            existing.id, existing.edi_invoice_no
        )));
    }

    let amount_mismatch = existing.total as i64 != edi_total;

    sqlx::query(
        "UPDATE t_billing_invoice SET edi_invoice_no = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(existing.id)
    .bind(edi_invoice_no)
    .execute(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("請求書更新エラー: {e}")))?;

    if amount_mismatch {
        // 金額不一致はリンクした上で要確認に倒す（金額を自動的に上書きはしない）
        return Err(PhaseError::Permanent(format!(
            "請求書ID={}と突合しましたが金額が不一致です（自社:¥{}, 先方:¥{edi_total}）",
            existing.id, existing.total
        )));
    }

    tracing::info!(
        "[Phase3] 請求書突合完了: invoice_id={}, client_id={client_id}, target_month={target_month}",
        existing.id
    );

    let client_name: String = sqlx::query_scalar("SELECT name FROM m_client WHERE id = $1")
        .bind(client_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("client_id={client_id}"));
    let label = format!(
        "{}（{}, EDI請求番号={}）",
        client_name,
        target_month.format("%Y年%m月"),
        edi_invoice_no
    );
    Ok(RegisterOutcome::Registered(RegisteredItem::Invoice(label)))
}

// ============================================================
// ATTACHMENT Excel/PDF — 稼働報告書（勤務表）
// ============================================================

async fn register_timesheet_attachment(
    pool: &PgPool,
    row: &FetchedEmail,
    raw_bytes: &[u8],
) -> Result<RegisterOutcome, PhaseError> {
    let parsed = crate::domain::services::excel_parser::auto_detect_and_parse(
        raw_bytes,
        &row.attachment_filename,
    );

    if let Some(err) = &parsed.error {
        return Err(PhaseError::Permanent(format!("稼働報告解析失敗: {err}")));
    }
    let target_month = parsed
        .target_month
        .ok_or_else(|| PhaseError::Permanent("稼働報告から対象月を特定できませんでした".to_string()))?;
    if parsed.worker_name.trim().is_empty() {
        return Err(PhaseError::Permanent("稼働報告から作業者名を特定できませんでした".to_string()));
    }

    // 既存エンジニアのみ照合（空白・全角スペースを無視して比較）
    let name_key = normalize_engineer_name_key(&parsed.worker_name);
    let engineer_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_engineer WHERE REPLACE(REPLACE(name, '　', ''), ' ', '') = $1",
    )
    .bind(&name_key)
    .fetch_optional(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("社員検索エラー: {e}")))?;
    let Some(engineer_id) = engineer_id else {
        return Err(PhaseError::Permanent(format!(
            "作業者名'{}'に一致する社員が見つかりません",
            parsed.worker_name
        )));
    };

    // 対象月にアクティブな契約を検索。一意に決まらない場合は自動登録せず要確認に倒す
    // （同一エンジニアが複数クライアントと同時契約しているケースは人手での確認が必要なため）。
    let month_end = month_end_date(target_month);
    let contract_ids: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client_contract WHERE engineer_id = $1 AND start_date <= $2 AND end_date >= $3",
    )
    .bind(engineer_id)
    .bind(month_end)
    .bind(target_month)
    .fetch_all(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("契約検索エラー: {e}")))?;

    let contract_id = match contract_ids.as_slice() {
        [id] => *id,
        [] => {
            return Err(PhaseError::Permanent(format!(
                "エンジニア'{}'の{target_month}時点で有効な契約が見つかりません",
                parsed.worker_name
            )))
        }
        _ => {
            return Err(PhaseError::Permanent(format!(
                "エンジニア'{}'の{target_month}時点で契約が複数({}件)あり自動決定できません",
                parsed.worker_name,
                contract_ids.len()
            )))
        }
    };

    let settlement_overtime = {
        use crate::domain::value_objects::SettlementTerms;
        use crate::infrastructure::repositories::order_repo;
        let contract = order_repo::find_client_contract_for_order(pool, contract_id)
            .await
            .map_err(|e| PhaseError::Transient(format!("契約取得エラー: {e}")))?
            .ok_or_else(|| {
                PhaseError::Permanent(format!("契約 id={contract_id} が見つかりません"))
            })?;
        let terms = SettlementTerms {
            lower_limit_hours: contract.lower_limit_hours,
            upper_limit_hours: contract.upper_limit_hours,
            fixed_hours: contract.fixed_hours,
            deduction_rate: contract.deduction_rate,
            overtime_rate: contract.overtime_rate,
        };
        terms.excess_hours(parsed.total_hours)
    };

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_monthly_timesheet WHERE client_contract_id = $1 AND target_month = $2)",
    )
    .bind(contract_id)
    .bind(target_month)
    .fetch_one(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("重複チェックエラー: {e}")))?;

    if exists {
        return Ok(RegisterOutcome::AlreadyExists);
    }

    let daily_data = serde_json::to_value(&parsed.daily_data).unwrap_or(serde_json::Value::Null);
    let alerts_json = serde_json::to_value(&parsed.alerts).unwrap_or(serde_json::Value::Null);

    sqlx::query(
        r#"
        INSERT INTO t_monthly_timesheet (
            client_contract_id, target_month, status, total_hours, work_days,
            overtime_hours, night_hours, holiday_hours,
            daily_data, original_filename, alerts_json, uploaded_at
        ) VALUES ($1, $2, 'UPLOADED', $3, $4, $5, $6, $7, $8, $9, $10, NOW())
        "#,
    )
    .bind(contract_id)
    .bind(target_month)
    .bind(parsed.total_hours)
    .bind(parsed.work_days)
    .bind(settlement_overtime)
    .bind(rust_decimal::Decimal::ZERO)
    .bind(rust_decimal::Decimal::ZERO)
    .bind(&daily_data)
    .bind(&row.attachment_filename)
    .bind(&alerts_json)
    .execute(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("稼働報告INSERTエラー: {e}")))?;

    // 報告受領: 発注/受注を REPORT_RECEIVED へ（失敗しても稼働報告登録は成功扱い）
    match crate::infrastructure::repositories::order_repo::advance_to_report_received_for_timesheet(
        pool,
        contract_id,
        target_month,
    )
    .await
    {
        Ok((po_n, ro_n)) if po_n > 0 || ro_n > 0 => {
            tracing::info!(
                "[Phase3] 報告受領ステータス更新: purchase_orders={po_n}, received_orders={ro_n}, contract_id={contract_id}, target_month={target_month}"
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(
                "[Phase3] 報告受領ステータス更新失敗（稼働報告は登録済み）: contract_id={contract_id}, target_month={target_month}, err={e}"
            );
        }
    }

    tracing::info!(
        "[Phase3] 稼働報告登録完了: contract_id={contract_id}, target_month={target_month}, worker={}",
        parsed.worker_name
    );

    let label = format!("{}（{}）", parsed.worker_name, target_month.format("%Y年%m月"));
    Ok(RegisterOutcome::Registered(RegisteredItem::Timesheet(label)))
}

fn normalize_engineer_name_key(name: &str) -> String {
    name.replace('\u{3000}', "").replace(' ', "")
}

fn month_end_date(month_start: NaiveDate) -> NaiveDate {
    let next_month = if month_start.month() == 12 {
        NaiveDate::from_ymd_opt(month_start.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month_start.year(), month_start.month() + 1, 1)
    };
    next_month.and_then(|d| d.pred_opt()).unwrap_or(month_start)
}
