/// infrastructure/mail_pipeline/phase1_watch.rs — Phase1: メール監視・条件判定
///
/// 責務: last_processed_at 以降のメールをチェックポイント方式で走査し、
///       EDI_API / ATTACHMENT / IGNORED に分類して t_received_email に保存するだけ。
///       EDI_API は、取引先マスタの「EDI通知メール」欄のアドレスから届いた通知メール。
///       添付バイナリの取得やEDI APIへのアクセスは行わない（Phase2の責務）。
///
/// 「未読/既読」に依存しない設計:
/// - IMAP は EXAMINE(=PEEK) で開くため既読フラグを変更しない。
/// - どのメールを処理済みとするかは s_mail_sync_checkpoint.last_processed_at と
///   t_received_email.message_id の UNIQUE 制約のみで判断する。

use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use super::checkpoint;
use super::imap_util::{
    decode_mime_string, extract_attachment_filenames, extract_body_text_from_parts, fetch_since_blocking,
    get_header_value, parse_email_date, parse_from_header, strip_nul, ImapConfig,
};
use super::ops_message;
use crate::domain::models::mail_pipeline::{PipelineStatus, SourceType};

/// Phase1実行結果
#[derive(Debug, Default)]
pub struct Phase1Result {
    pub scanned: usize,
    pub saved: usize,
    pub skipped_duplicate: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for Phase1Result {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "走査:{}件, 新規保存:{}件, 重複スキップ:{}件, エラー:{}件",
            self.scanned, self.saved, self.skipped_duplicate, self.errors.len()
        )
    }
}

/// Phase1エントリポイント: チェックポイント以降のメールを走査・分類・保存する
///
/// 個別メールの解析失敗はスキップしてログに残すのみ（1件の異常で全体を止めない）。
/// IMAP接続自体の失敗は Err で返し、呼び出し元(オーケストレーター)が
/// 致命的エラーとして即時アラートを送る。
pub async fn run(pool: &PgPool) -> Result<Phase1Result, String> {
    let mut result = Phase1Result::default();

    // ローカル開発環境では本番/ステージングのメールアカウントへ誤接続しないよう、
    // IMAP同期処理そのものをデフォルトでスキップする（ENV_NAME未設定時もlocal扱い）。
    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "local".to_string());
    if env_name == "local" {
        tracing::info!("[Phase1] ENV_NAME=local のためIMAP同期をスキップします（本番メールアカウントへの誤接続防止）");
        return Ok(result);
    }

    let config = ImapConfig::load(pool).await?;
    let mailbox = config.mailbox_id().to_string();

    // バッチ実行「開始」時刻を記録する（完了時刻ではない）。
    // 実行中に届いた新着メールを次回の安全マージンで確実に拾うため。
    let run_started_at = chrono::Utc::now();

    let last_processed_at = checkpoint::get_last_processed_at(pool, &mailbox).await;
    let since = checkpoint::search_since_date(last_processed_at);
    let since_str = since.format("%d-%b-%Y").to_string();

    tracing::info!(
        "[Phase1] メール走査開始: mailbox={mailbox}, last_processed_at={last_processed_at}, SINCE={since_str}"
    );

    // IMAP通信(接続・検索・取得)は同期処理のため、Tokioのワーカースレッドをブロックしないよう
    // spawn_blocking上でまとめて実行する（#073再発防止、開発標準書§2.3）。
    // connect_and_examine自体のタイムアウトはimap_util::IMAP_TIMEOUT（30秒）で保護済み。
    let raw_messages = tokio::task::spawn_blocking(move || fetch_since_blocking(&config, &since_str))
        .await
        .map_err(|e| format!("メール読取タスクが異常終了しました: {e}"))??;

    if raw_messages.is_empty() {
        checkpoint::update_checkpoint(pool, &mailbox, run_started_at)
            .await
            .map_err(|e| {
                format!(
                    "チェックポイント更新失敗: {}",
                    ops_message::format_sqlx(&e)
                )
            })?;
        tracing::info!("[Phase1] 対象メールなし");
        return Ok(result);
    }

    for raw in &raw_messages {
        result.scanned += 1;

        let header_bytes = raw.header.as_slice();
        let body_bytes = raw.body.as_slice();

        let parsed_header = match mailparse::parse_headers(header_bytes) {
            Ok((headers, _)) => headers,
            Err(e) => {
                result.errors.push(ops_message::fail(
                    "メール取込",
                    "Phase1",
                    "メールヘッダー解析",
                    "当該メールをスキップ（他メールの読取・取込は継続）",
                    e,
                ));
                continue;
            }
        };

        let message_id = strip_nul(
            get_header_value(&parsed_header, "Message-ID")
                .unwrap_or_default()
                .trim_matches(|c| c == '<' || c == '>' || c == ' '),
        );

        if message_id.is_empty() {
            continue;
        }

        // 冪等性: message_id の重複はここで確定判定する
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM t_received_email WHERE message_id = $1)",
        )
        .bind(&message_id)
        .fetch_one(pool)
        .await
        .unwrap_or(true);

        if exists {
            result.skipped_duplicate += 1;
            continue;
        }

        let from_raw = get_header_value(&parsed_header, "From").unwrap_or_default();
        let subject_raw = get_header_value(&parsed_header, "Subject").unwrap_or_default();
        let date_raw = get_header_value(&parsed_header, "Date").unwrap_or_default();

        // DB(text列)は NUL 文字を保存できないため、保存する文字列はすべて strip_nul を通す
        let (from_name, from_email) = parse_from_header(&from_raw);
        let (from_name, from_email) = (strip_nul(&from_name), strip_nul(&from_email));
        let subject = strip_nul(&decode_mime_string(&subject_raw));
        let body_text = strip_nul(&extract_body_text_from_parts(header_bytes, body_bytes));

        let all_bytes = [header_bytes, body_bytes].concat();
        let attachment_names: Vec<String> = extract_attachment_filenames(&all_bytes)
            .iter()
            .map(|n| strip_nul(n))
            .collect();
        let attachment_str = attachment_names.join(", ");

        let received_at = parse_email_date(&date_raw);

        // ── 分類: EDI_API / ATTACHMENT / IGNORED ──
        let classification =
            classify(pool, &from_email, !attachment_names.is_empty()).await;
        let source_type = classification.source_type;

        let status = if source_type == SourceType::Ignored {
            PipelineStatus::Ignored
        } else {
            PipelineStatus::New
        };

        let save_result = sqlx::query(
            r#"
            INSERT INTO t_received_email (
                message_id, from_email, from_name, subject, received_at,
                body_text, partner_id, client_id, status,
                attachment_filename, attachment_file, source_type
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, '', $11)
            ON CONFLICT (message_id) DO NOTHING
            "#,
        )
        .bind(&message_id)
        .bind(&from_email)
        .bind(&from_name)
        .bind(&subject)
        .bind(received_at)
        .bind(&body_text)
        .bind(classification.partner_id.clone())
        .bind(classification.client_id)
        .bind(status.as_str())
        .bind(&attachment_str)
        .bind(source_type.as_str())
        .execute(pool)
        .await;

        match save_result {
            Ok(r) if r.rows_affected() > 0 => {
                result.saved += 1;
                tracing::info!(
                    "[Phase1] 保存: {from_email} | {subject} | source_type={source_type} | attachments=[{attachment_str}]"
                );

                // ダッシュボード表示用の t_mail_scan_log にも同期（既存挙動を踏襲）
                if let Some(err) = sync_mail_scan_log(
                    pool, &message_id, &from_email, &from_name, &subject,
                    received_at, &body_text, &attachment_names,
                    classification.client_name.as_deref(), classification.partner_id,
                ).await {
                    result.errors.push(err);
                }
            }
            Ok(_) => result.skipped_duplicate += 1,
            Err(e) => result.errors.push(ops_message::fail(
                "メール取込",
                "Phase1",
                "メール本体の保存(t_received_email)",
                "当該メール未取込（他メールの処理は継続）",
                format!("message_id={message_id} | {}", ops_message::format_sqlx(&e)),
            )),
        }
    }

    checkpoint::update_checkpoint(pool, &mailbox, run_started_at)
        .await
        .map_err(|e| {
            format!(
                "チェックポイント更新失敗: {}",
                ops_message::format_sqlx(&e)
            )
        })?;

    tracing::info!("[Phase1] 完了: {result}");
    Ok(result)
}

/// 分類結果。client_id は m_client 照合時のみ Some（partner_idとは排他）。
struct Classification {
    source_type: SourceType,
    partner_id: Option<String>,
    client_id: Option<i64>,
    client_name: Option<String>,
}

/// メールをEDI_API/ATTACHMENT/IGNOREDに分類する
async fn classify(
    pool: &PgPool,
    from_email: &str,
    has_attachment: bool,
) -> Classification {
    // 取引先 EDI の通知メールか（差出人の照合は、取引先ごとのカスタマイズが行う）。
    // 該当すれば、Phase2 がカスタマイズの中で、件名・本文を見て書類を取るか（取引先の EDI へ接続するか）を決める。
    if let Some(sender) = crate::custom::edi_notice_sender(pool, from_email).await {
        return Classification {
            source_type: SourceType::EdiApi,
            partner_id: None,
            client_id: Some(sender.client_id),
            client_name: Some(sender.client_name),
        };
    }

    // m_partner の主キーは partner_id VARCHAR(32)（id列は存在しない）。
    // 旧email_fetcher.rsは "SELECT id FROM m_partner" という誤ったクエリで
    // 常にエラー→.ok()で握りつぶされ、パートナー照合が機能していなかった。
    let partner_id: Option<String> = sqlx::query_scalar(
        "SELECT partner_id FROM m_partner WHERE LOWER(email) = LOWER($1) LIMIT 1",
    )
    .bind(from_email)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    #[derive(sqlx::FromRow)]
    struct ClientMatch {
        id: i64,
        name: String,
    }
    let client_match: Option<ClientMatch> = sqlx::query_as(
        "SELECT id, name FROM m_client WHERE LOWER(email) = LOWER($1) LIMIT 1",
    )
    .bind(from_email)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    // ドメイン一致の判定（DB アクセス失敗は未照合扱いで続行）
    let domain_matches = crate::infrastructure::repositories::received_email_repo::sender_matches_known_domain(pool, from_email)
        .await
        .log_err().unwrap_or(false);

    let recognized = partner_id.is_some() || client_match.is_some() || domain_matches;
    let client_id = client_match.as_ref().map(|c| c.id);
    let client_name = client_match.map(|c| c.name);

    let source_type = attachment_source_type(has_attachment, recognized);
    if recognized {
        Classification { source_type, partner_id, client_id, client_name }
    } else {
        // 未照合の場合は partner_id/client_id を持ち込まない
        Classification { source_type, partner_id: None, client_id: None, client_name: None }
    }
}

/// 添付付きメールを取込対象(ATTACHMENT)にするかを決める。
///
/// 送信元（アドレスまたはドメイン）が登録済みの取引先/パートナーと一致する場合だけ取り込む。
/// 以前は件名キーワード（「請求書」等）だけでも取り込んでいたため、自社宛のSaaS利用料の請求メール
/// （例: payments-noreply@google.com の Google Workspace 請求書）が取引先不明で毎月
/// PARSE_FAILED・要確認通知になっていた。未照合のメールは IGNORED として記録だけ残す
/// （ダッシュボードのメール取込一覧で後から確認できる）。
fn attachment_source_type(has_attachment: bool, sender_recognized: bool) -> SourceType {
    if has_attachment && sender_recognized {
        SourceType::Attachment
    } else {
        SourceType::Ignored
    }
}

/// t_mail_scan_log への同期（ダッシュボードのメールチェック一覧表示用、既存挙動を踏襲）
///
/// 失敗してもメール本体(t_received_email)の取込は済んでいる。呼び出し元で errors に積む。
async fn sync_mail_scan_log(
    pool: &PgPool,
    message_id: &str,
    from_email: &str,
    from_name: &str,
    subject: &str,
    received_at: chrono::DateTime<chrono::Utc>,
    body_text: &str,
    attachment_names: &[String],
    matched_client: Option<&str>,
    partner_id: Option<String>,
) -> Option<String> {
    let classification = classify_subject_label(subject);
    let matched_name = if let Some(name) = matched_client {
        name.to_string()
    } else if let Some(pid) = partner_id {
        sqlx::query_scalar::<_, String>("SELECT name FROM m_partner WHERE partner_id = $1")
            .bind(pid)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .unwrap_or_default()
    } else {
        String::new()
    };

    // attachments は JSONB 列のため、カンマ区切り文字列ではなく JSON 配列として渡す必要がある
    // (以前は単純な文字列を直接バインドしており、Postgres側で
    // "invalid input syntax for type json" となって毎回保存に失敗していた)
    let attachments_json = serde_json::json!(attachment_names);

    // 1件の記録失敗が他メールのスキャンを止めるべきではないため処理は継続するが、ログは残す
    if let Err(e) = sqlx::query(
        r#"INSERT INTO t_mail_scan_log (
            gmail_message_id, sender_email, sender_name, subject,
            received_at, classification, matched_entity_name, body_text, attachments
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (gmail_message_id) DO NOTHING"#,
    )
    .bind(message_id)
    .bind(from_email)
    .bind(from_name)
    .bind(subject)
    .bind(received_at)
    .bind(&classification)
    .bind(&matched_name)
    .bind(body_text)
    .bind(&attachments_json)
    .execute(pool)
    .await
    {
        return Some(ops_message::fail(
            "メール取込",
            "Phase1",
            "メールチェック一覧への書き込み(t_mail_scan_log)",
            "ダッシュボード一覧に出ない（メール本体の読取・取込は成功）",
            format!("message_id={message_id} | {}", ops_message::format_sqlx(&e)),
        ));
    }
    None
}

pub(crate) fn classify_subject_label(subject: &str) -> String {
    if subject.contains("注文書") || subject.contains("発注") || subject.contains("受注") {
        "ORDER".to_string()
    } else if subject.contains("稼働報告") || subject.contains("作業報告") || subject.contains("勤怠") || subject.contains("報告書") || subject.contains("勤務表") {
        "REPORT".to_string()
    } else if subject.contains("請求書") || subject.contains("支払通知") {
        "INVOICE".to_string()
    } else if subject.contains("契約") || subject.contains("承認") {
        "CONTRACT".to_string()
    } else {
        "OTHER".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrecognized_sender_with_attachment_is_ignored() {
        // Google Workspace の請求メール（件名に「請求書」、PDF添付あり、送信元は取引先外）
        assert_eq!(attachment_source_type(true, false), SourceType::Ignored);
    }

    #[test]
    fn recognized_sender_with_attachment_is_imported() {
        assert_eq!(attachment_source_type(true, true), SourceType::Attachment);
    }

    #[test]
    fn mail_without_attachment_is_ignored() {
        assert_eq!(attachment_source_type(false, true), SourceType::Ignored);
        assert_eq!(attachment_source_type(false, false), SourceType::Ignored);
    }
}
