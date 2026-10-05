/// infrastructure/mail_pipeline/phase2_fetch.rs — Phase2: データソース取得
///
/// 責務: 実データを取得し、`t_received_email` の行を `FETCHED`（成功）または
///       `FETCH_FAILED`（失敗・リトライ対象）に更新するだけ。パース・DB登録はPhase3の責務。
///
/// 2種類の取得経路を独立に実行する:
///
/// 1. 取引先 EDI の通知メール（`source_type='EDI_API'`）
///    取引先ごとのカスタマイズ（`crate::custom`）に任せる。取引先の EDI へ接続するか・何を取るかは、
///    カスタマイズの中で、届いた通知メールの内容から決める。取得した書類は `FETCHED` の行になり、
///    Phase3 が登録する。カスタマイズが無いビルドでは、何もしない。
///
/// 2. 添付ファイル系メール（`status='NEW' AND source_type='ATTACHMENT'`）
///    Phase1が見つけた行ごとに、IMAPで該当メールを再取得（EXAMINEで再フェッチ、既読状態は
///    変更しない）し、添付バイナリを実体化して `raw_attachment` 列に保存する。

use sqlx::PgPool;

use super::imap_util::{fetch_attachments_blocking, Attachment, ImapConfig};
use super::ops_message;
use crate::domain::models::mail_pipeline::MAX_RETRY;

/// Phase2実行結果
#[derive(Debug, Default)]
pub struct Phase2Result {
    pub edi_notices: usize,
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
            "EDI通知メール{}件(新規注文{}件/新規請求{}件), 添付取得{}件成功/{}件失敗, エラー{}件",
            self.edi_notices, self.edi_new_orders, self.edi_new_invoices,
            self.attachment_fetched, self.attachment_failed, self.errors.len()
        )
    }
}

/// Phase2エントリポイント
pub async fn run(pool: &PgPool) -> Phase2Result {
    let mut result = Phase2Result::default();

    let edi = crate::custom::fetch_edi_notice_documents(pool).await;
    result.edi_notices = edi.notices;
    result.edi_new_orders = edi.new_orders;
    result.edi_new_invoices = edi.new_invoices;
    result.errors.extend(edi.errors);
    fetch_attachment_sources(pool, &mut result).await;

    tracing::info!("[Phase2] 完了: {result}");
    result
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
            Ok(attachments) => save_attachments(pool, email.id, &attachments).await,
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

/// 添付バイナリをDBへ保存する（IMAP取得は `fetch_attachments_blocking`（spawn_blocking経由）で完了済み）。
///
/// - `t_received_email.attachment_filename / raw_attachment` には「主の添付」を保存する（受注書・請求書PDFの取込が使う）。
/// - 書類として読める添付は、すべて `t_received_email_attachment` に1件ずつ保存する
///   （1通に複数名分の勤務表が付いているケース対応）。
async fn save_attachments(pool: &PgPool, email_id: i64, attachments: &[Attachment]) -> Result<(), String> {
    let primary = super::imap_util::pick_primary_attachment(attachments)
        .ok_or_else(|| "添付ファイルが見つかりませんでした".to_string())?;

    let mut tx = pool.begin().await.map_err(|e| format!("DBトランザクション開始エラー: {e}"))?;
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
    .bind(&primary.filename)
    .bind(&primary.bytes)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("DB更新エラー: {e}"))?;

    for (seq, a) in attachments
        .iter()
        .filter(|a| super::imap_util::is_document_filename(&a.filename))
        .enumerate()
    {
        sqlx::query(
            r#"
            INSERT INTO t_received_email_attachment (email_id, seq, filename, content_type, content)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (email_id, seq) DO NOTHING
            "#,
        )
        .bind(email_id)
        .bind(seq as i32)
        .bind(&a.filename)
        .bind(&a.content_type)
        .bind(&a.bytes)
        .execute(&mut *tx)
        .await
        .map_err(|e| format!("添付保存エラー: {e}"))?;
    }
    crate::infrastructure::db_tx::commit_checked(tx).await.map_err(|e| format!("DBコミットエラー: {e}"))?;

    tracing::info!(
        "[Phase2] 添付取得成功: メールID={email_id}, 主={}, 書類{}件/全{}件",
        primary.filename,
        attachments.iter().filter(|a| super::imap_util::is_document_filename(&a.filename)).count(),
        attachments.len()
    );
    Ok(())
}

/// 取得失敗をFETCH_FAILEDとして記録し、リトライ回数に応じてnext_retry_at/needs_manual_reviewを更新する
pub(crate) async fn mark_fetch_failed(pool: &PgPool, email_id: i64, current_retry_count: i32, error: &str) {
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
