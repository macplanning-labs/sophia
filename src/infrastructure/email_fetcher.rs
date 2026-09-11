/// infrastructure/email_fetcher.rs — 受信メール自動取得
///
/// EDI_MP の invoices/services/email_receiver.py をRustに移植。
///
/// ## 処理フロー
/// 1. IMAP4_SSL で Gmail に接続（PEEK — 未読フラグ変更なし）
/// 2. SINCE 過去7日 のメールを検索（未読/既読問わず）
/// 3. message_id で DB 照合（処理済みはスキップ）
/// 4. 件名フィルタ（キーワードマッチ）
/// 5. 送信元 → m_partner / m_client で照合
/// 6. t_received_email に保存
///
/// ## 2度読み防止
/// - 未読フラグに依存しない（EDI_MP と並行稼働可能）
/// - `t_received_email.message_id` の UNIQUE 制約で重複防止

use sqlx::PgPool;
use tracing;

// 件名に含まれるキーワード（いずれかがマッチすれば対象）
const SUBJECT_KEYWORDS: &[&str] = &[
    "稼働報告", "作業報告", "請求書", "勤怠", "報告書",
    "支払通知", "承認", "注文書", "発注", "受注",
];

/// メール取得結果
#[derive(Debug, Default)]
pub struct FetchResult {
    pub processed: usize,
    pub saved: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for FetchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "処理: {}件, 保存: {}件, スキップ: {}件, エラー: {}件",
            self.processed, self.saved, self.skipped, self.errors.len()
        )
    }
}

/// メインエントリポイント: IMAP接続 → メール取得 → DB保存
pub async fn fetch_and_process_emails(pool: &PgPool) -> FetchResult {
    let mut result = FetchResult::default();

    // 環境変数から IMAP 設定を取得
    let imap_host = std::env::var("IMAP_HOST")
        .unwrap_or_else(|_| "imap.gmail.com".to_string());
    let imap_port: u16 = std::env::var("IMAP_PORT")
        .unwrap_or_else(|_| "993".to_string())
        .parse()
        .unwrap_or(993);
    let imap_user = std::env::var("IMAP_USER")
        .or_else(|_| std::env::var("EMAIL_HOST_USER"))
        .unwrap_or_default();
    let imap_password = std::env::var("IMAP_PASSWORD")
        .or_else(|_| std::env::var("EMAIL_HOST_PASSWORD"))
        .unwrap_or_default();

    if imap_user.is_empty() || imap_password.is_empty() {
        result.errors.push("IMAP認証情報が設定されていません（IMAP_USER/IMAP_PASSWORD）".into());
        return result;
    }

    tracing::info!("[メール取得] IMAP接続開始: {}:{} (user={})", imap_host, imap_port, imap_user);

    // IMAP接続（TLS）
    let tls = match native_tls::TlsConnector::new() {
        Ok(t) => t,
        Err(e) => {
            result.errors.push(format!("TLS初期化エラー: {e}"));
            return result;
        }
    };

    let client = match imap::connect(
        (imap_host.as_str(), imap_port),
        &imap_host,
        &tls,
    ) {
        Ok(c) => c,
        Err(e) => {
            result.errors.push(format!("IMAP接続エラー: {e}"));
            return result;
        }
    };

    let mut session = match client.login(&imap_user, &imap_password) {
        Ok(s) => s,
        Err((e, _)) => {
            result.errors.push(format!("IMAPログインエラー: {e}"));
            return result;
        }
    };

    // INBOX を読み取り専用で開く（EXAMINE = PEEK、既読フラグを変更しない）
    if let Err(e) = session.examine("INBOX") {
        result.errors.push(format!("INBOX EXAMINE エラー: {e}"));
        let _ = session.logout();
        return result;
    }

    // 過去7日間のメールを検索（未読/既読問わず）
    let since_date = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::days(7))
        .unwrap_or_else(chrono::Utc::now);
    let since_str = since_date.format("%d-%b-%Y").to_string();

    let search_result = match session.search(format!("SINCE {since_str}")) {
        Ok(ids) => ids,
        Err(e) => {
            result.errors.push(format!("メール検索エラー: {e}"));
            let _ = session.logout();
            return result;
        }
    };

    tracing::info!("[メール取得] 過去7日のメール: {}件", search_result.len());

    if search_result.is_empty() {
        let _ = session.logout();
        return result;
    }

    // UID リストをカンマ区切りに
    let uid_set: Vec<String> = search_result.iter().map(|id| id.to_string()).collect();
    let uid_str = uid_set.join(",");

    // メールヘッダーのみ取得（BODY.PEEK で既読フラグを変更しない）
    let messages = match session.fetch(
        &uid_str,
        "(BODY.PEEK[HEADER] BODY.PEEK[TEXT] FLAGS)",
    ) {
        Ok(m) => m,
        Err(e) => {
            result.errors.push(format!("メール取得エラー: {e}"));
            let _ = session.logout();
            return result;
        }
    };

    for msg in messages.iter() {
        result.processed += 1;

        // ヘッダー解析
        let header_bytes = match msg.header() {
            Some(h) => h,
            None => continue,
        };
        let body_bytes = msg.text().unwrap_or(b"");

        let parsed_header = match mailparse::parse_headers(header_bytes) {
            Ok((headers, _)) => headers,
            Err(e) => {
                result.errors.push(format!("ヘッダー解析エラー: {e}"));
                continue;
            }
        };

        // Message-ID 取得
        let message_id = get_header_value(&parsed_header, "Message-ID")
            .unwrap_or_default()
            .trim_matches(|c| c == '<' || c == '>' || c == ' ')
            .to_string();

        if message_id.is_empty() {
            result.skipped += 1;
            continue;
        }

        // DB重複チェック
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM t_received_email WHERE message_id = $1)"
        )
        .bind(&message_id)
        .fetch_one(pool)
        .await
        .unwrap_or(true);

        if exists {
            result.skipped += 1;
            continue;
        }

        // ヘッダーフィールド取得
        let from_raw = get_header_value(&parsed_header, "From").unwrap_or_default();
        let subject_raw = get_header_value(&parsed_header, "Subject").unwrap_or_default();
        let date_raw = get_header_value(&parsed_header, "Date").unwrap_or_default();

        // From ヘッダー解析
        let (from_name, from_email) = parse_from_header(&from_raw);

        // Subject デコード
        let subject = decode_mime_string(&subject_raw);

        // 本文取得（text/plain、最大5000文字）
        let body_text = extract_body_text(body_bytes);

        // 添付ファイル名リスト取得
        let all_bytes = [header_bytes, body_bytes].concat();
        let attachments = extract_attachment_filenames(&all_bytes);
        let attachment_str = attachments.join(", ");

        // 件名フィルタ
        let matches_filter = matches_subject_filter(&subject);

        // 送信元照合（パートナー / クライアント）
        let partner_id = match_partner(pool, &from_email).await;
        let client_name = match_client(pool, &from_email).await;

        // ステータス判定
        let status = if !matches_filter && partner_id.is_none() && client_name.is_none() {
            "IGNORED"
        } else {
            "NEW"
        };

        // 日時パース
        let received_at = parse_email_date(&date_raw);

        // DB保存
        let save_result = sqlx::query(
            r#"
            INSERT INTO t_received_email (
                message_id, from_email, from_name, subject, received_at,
                body_text, partner_id, status, error_message,
                attachment_filename, attachment_file
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, '')
            ON CONFLICT (message_id) DO NOTHING
            "#
        )
        .bind(&message_id)
        .bind(&from_email)
        .bind(&from_name)
        .bind(&subject)
        .bind(received_at)
        .bind(&body_text)
        .bind(partner_id.map(|id| id.to_string()))
        .bind(status)
        .bind(if client_name.is_some() {
            format!("クライアント: {}", client_name.as_deref().unwrap_or(""))
        } else {
            String::new()
        })
        .bind(&attachment_str)
        .execute(pool)
        .await;

        match save_result {
            Ok(r) if r.rows_affected() > 0 => {
                result.saved += 1;
                tracing::info!(
                    "[メール取得] 保存: {} | {} | {} | status={}",
                    from_email, subject, attachment_str, status
                );

                // t_mail_scan_log にも同期（ダッシュボードのメールチェック一覧に表示するため）
                let classification = classify_subject(&subject);
                let matched_name = if let Some(ref name) = client_name {
                    name.clone()
                } else {
                    // パートナー名を取得
                    if let Some(pid) = partner_id {
                        sqlx::query_scalar::<_, String>(
                            "SELECT name FROM m_partner WHERE id = $1"
                        )
                        .bind(pid)
                        .fetch_optional(pool)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_default()
                    } else {
                        String::new()
                    }
                };

                let _ = sqlx::query(
                    r#"INSERT INTO t_mail_scan_log (
                        gmail_message_id, sender_email, sender_name, subject,
                        received_at, classification, matched_entity_name, body_text, attachments
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                    ON CONFLICT (gmail_message_id) DO NOTHING"#
                )
                .bind(&message_id)
                .bind(&from_email)
                .bind(&from_name)
                .bind(&subject)
                .bind(received_at)
                .bind(&classification)
                .bind(&matched_name)
                .bind(&body_text)
                .bind(&attachment_str)
                .execute(pool)
                .await;
            }
            Ok(_) => {
                // ON CONFLICT DO NOTHING — 既に存在
                result.skipped += 1;
            }
            Err(e) => {
                result.errors.push(format!("DB保存エラー ({}): {e}", message_id));
            }
        }
    }

    let _ = session.logout();

    tracing::info!("[メール取得] 完了: {result}");
    result
}


// ============================================================
// メール分類
// ============================================================

/// 件名からメールの分類を判定
fn classify_subject(subject: &str) -> String {
    if subject.contains("注文書") || subject.contains("発注") || subject.contains("受注") {
        "ORDER".to_string()
    } else if subject.contains("稼働報告") || subject.contains("作業報告") || subject.contains("勤怠") || subject.contains("報告書") {
        "REPORT".to_string()
    } else if subject.contains("請求書") || subject.contains("支払通知") {
        "INVOICE".to_string()
    } else if subject.contains("契約") || subject.contains("承認") {
        "CONTRACT".to_string()
    } else {
        "OTHER".to_string()
    }
}

// ============================================================
// ヘッダー解析ユーティリティ
// ============================================================

/// ヘッダーから指定フィールドの値を取得
fn get_header_value(headers: &[mailparse::MailHeader], name: &str) -> Option<String> {
    headers.iter()
        .find(|h| h.get_key().eq_ignore_ascii_case(name))
        .map(|h| h.get_value())
}

/// From ヘッダーから名前とメールアドレスを抽出
///
/// パターン: "名前 <email@example.com>" → ("名前", "email@example.com")
fn parse_from_header(from_raw: &str) -> (String, String) {
    let decoded = decode_mime_string(from_raw);

    // "名前 <email>" パターン
    if let Some(lt) = decoded.find('<') {
        if let Some(gt) = decoded.find('>') {
            let name = decoded[..lt].trim().trim_matches('"').to_string();
            let email = decoded[lt + 1..gt].trim().to_string();
            return (name, email);
        }
    }

    // メールアドレスのみ
    let addr = decoded.trim().trim_matches(|c| c == '<' || c == '>').to_string();
    (String::new(), addr)
}

/// MIMEエンコード文字列をデコード
///
/// =?UTF-8?B?...?= や =?ISO-2022-JP?B?...?= をデコードする
fn decode_mime_string(s: &str) -> String {
    let mut result = s.to_string();

    // =?charset?encoding?data?= パターンを検索・デコード
    loop {
        let start = match result.find("=?") {
            Some(s) => s,
            None => break,
        };
        let after_start = &result[start + 2..];
        let end = match after_start.find("?=") {
            Some(e) => e,
            None => break,
        };
        let encoded_part = &result[start..start + 2 + end + 2];

        // "charset?encoding?data" を分割
        let inner = &encoded_part[2..encoded_part.len() - 2];
        let parts: Vec<&str> = inner.splitn(3, '?').collect();
        if parts.len() == 3 {
            let charset = parts[0];
            let encoding = parts[1];
            let data = parts[2];

            let decoded_bytes = match encoding.to_uppercase().as_str() {
                "B" => base64_decode(data),
                "Q" => quoted_printable_decode(data),
                _ => None,
            };

            if let Some(bytes) = decoded_bytes {
                let decoded_str = decode_charset(&bytes, charset);
                result = format!("{}{}{}", &result[..start], decoded_str, &result[start + 2 + end + 2..]);
                continue;
            }
        }
        // デコードできなかった場合はループ終了
        break;
    }

    result.trim().to_string()
}

/// Base64 デコード
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

/// Quoted-Printable デコード（簡易）
fn quoted_printable_decode(s: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""),
                16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'_' {
            result.push(b' '); // RFC 2047: _ = space
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    Some(result)
}

/// 文字セットからUTF-8に変換
fn decode_charset(bytes: &[u8], charset: &str) -> String {
    match charset.to_uppercase().as_str() {
        "UTF-8" | "UTF8" => String::from_utf8_lossy(bytes).to_string(),
        "ISO-2022-JP" => {
            // ISO-2022-JP → UTF-8 変換（簡易: lossy変換）
            String::from_utf8_lossy(bytes).to_string()
        }
        "SHIFT_JIS" | "SHIFT-JIS" | "SJIS" => {
            String::from_utf8_lossy(bytes).to_string()
        }
        _ => String::from_utf8_lossy(bytes).to_string(),
    }
}

/// メール本文（text/plain）を取得（最大5000文字）
fn extract_body_text(body_bytes: &[u8]) -> String {
    // BODY[TEXT] から text/plain を取得
    let text = String::from_utf8_lossy(body_bytes);

    // MIME マルチパートの場合は text/plain パートを探す
    if let Ok(parsed) = mailparse::parse_mail(body_bytes) {
        // マルチパートの場合
        if !parsed.subparts.is_empty() {
            for part in &parsed.subparts {
                let content_type = part.ctype.mimetype.to_lowercase();
                if content_type == "text/plain" {
                    if let Ok(body) = part.get_body() {
                        let truncated: String = body.chars().take(5000).collect();
                        return truncated;
                    }
                }
            }
        }
        // シングルパート
        if let Ok(body) = parsed.get_body() {
            let truncated: String = body.chars().take(5000).collect();
            return truncated;
        }
    }

    // フォールバック: 生バイトをそのまま
    let truncated: String = text.chars().take(5000).collect();
    truncated
}

/// メールから添付ファイル名を抽出
fn extract_attachment_filenames(raw_bytes: &[u8]) -> Vec<String> {
    let mut filenames = Vec::new();

    if let Ok(parsed) = mailparse::parse_mail(raw_bytes) {
        for part in &parsed.subparts {
            // Content-Disposition: attachment; filename="xxx" を検出
            let disposition = &part.ctype.params;
            if let Some(name) = disposition.get("name") {
                filenames.push(decode_mime_string(name));
            }

            // Content-Type の name パラメータもチェック
            if let Some(cd) = part.headers.iter().find(|h| h.get_key().eq_ignore_ascii_case("Content-Disposition")) {
                let value = cd.get_value();
                if value.contains("attachment") || value.contains("filename") {
                    if let Some(fname) = extract_filename_from_header(&value) {
                        filenames.push(fname);
                    }
                }
            }
        }
    }

    // 重複除去
    filenames.sort();
    filenames.dedup();
    filenames
}

/// Content-Disposition ヘッダーから filename を抽出
fn extract_filename_from_header(header_value: &str) -> Option<String> {
    // filename="xxx" または filename*=UTF-8''xxx パターン
    for part in header_value.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix("filename=") {
            return Some(rest.trim_matches('"').to_string());
        }
        if let Some(rest) = trimmed.strip_prefix("filename*=") {
            // RFC 5987: charset'language'value
            if let Some(value_start) = rest.find("''") {
                let encoded = &rest[value_start + 2..];
                // URLデコード
                let decoded = urlish_decode(encoded);
                return Some(decoded);
            }
        }
    }
    None
}

/// URLエンコード簡易デコード
fn urlish_decode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else {
            result.push(c);
        }
    }
    result
}

// ============================================================
// 件名フィルタ・送信元照合
// ============================================================

/// 件名がフィルタ条件に合致するかチェック
fn matches_subject_filter(subject: &str) -> bool {
    SUBJECT_KEYWORDS.iter().any(|kw| subject.contains(kw))
}

/// 送信元メールアドレスからパートナーIDを照合
async fn match_partner(pool: &PgPool, from_email: &str) -> Option<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT id FROM m_partner WHERE LOWER(email) = LOWER($1) LIMIT 1"
    )
    .bind(from_email)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// 送信元メールアドレスからクライアント名を照合
async fn match_client(pool: &PgPool, from_email: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT name FROM m_client WHERE LOWER(email) = LOWER($1) LIMIT 1"
    )
    .bind(from_email)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// メールの Date ヘッダーをパース
fn parse_email_date(date_str: &str) -> chrono::DateTime<chrono::Utc> {
    // mailparse の dateparse を使用
    match mailparse::dateparse(date_str) {
        Ok(timestamp) => {
            chrono::DateTime::from_timestamp(timestamp, 0)
                .unwrap_or_else(chrono::Utc::now)
        }
        Err(_) => chrono::Utc::now(),
    }
}
