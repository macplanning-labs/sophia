/// infrastructure/mail_pipeline/imap_util.rs — IMAP接続・MIME解析の共通ユーティリティ
///
/// Phase1(ヘッダーのみ走査・分類)とPhase2(添付バイナリ取得)の両方から使う。
/// email_fetcher.rs（旧実装）のMIME解析ロジックをそのまま移設したもの。

use mailparse::MailHeader;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

/// IMAPセッション（TLS）
pub type ImapSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;

/// IMAP接続・データ読み取りのタイムアウト。応答がない場合はこの秒数でErrを返し処理を抜ける
/// （2026-08-07、本番ダッシュボードAPIが504タイムアウトした障害の再発防止対応）。
const IMAP_TIMEOUT: Duration = Duration::from_secs(30);

/// 環境変数からIMAP設定を読み込む
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}

impl ImapConfig {
    /// IMAP接続情報を読み込む。ユーザー/パスワードはs_company_info（自社情報画面のSMTP設定）と
    /// 共通のGmailアカウント認証情報をそのまま使う。以前はIMAP_USER/IMAP_PASSWORD環境変数を
    /// 別途持っていたが、DB側のSMTPパスワード更新にこの環境変数が追従せず認証失敗＝IMAP自動
    /// ロックが再発する事故があったため、単一のソース（DB）に統一した（2026-08-18）。
    /// ホスト/ポートはGmail固定でほぼ変わらず機密情報でもないため、引き続き環境変数（未設定時は
    /// imap.gmail.com:993）を使う。
    pub async fn load(pool: &sqlx::PgPool) -> Result<Self, String> {
        let host = std::env::var("IMAP_HOST").unwrap_or_else(|_| "imap.gmail.com".to_string());
        let port: u16 = std::env::var("IMAP_PORT")
            .unwrap_or_else(|_| "993".to_string())
            .parse()
            .unwrap_or(993);

        let creds = crate::infrastructure::repositories::company_info_repo::get_smtp_credentials(pool)
            .await
            .map_err(|e| {
                // anyhow::Error の内部の sqlx::Error を抽出
                if let Some(sqlx_err) = e.downcast_ref::<sqlx::Error>() {
                    format!("自社情報のSMTP認証情報取得に失敗しました: {}", crate::infrastructure::mail_pipeline::ops_message::format_sqlx(sqlx_err))
                } else {
                    format!("自社情報のSMTP認証情報取得に失敗しました: {e}")
                }
            })?;

        let (user, password) = creds.ok_or_else(|| {
            "IMAP認証情報が設定されていません（自社情報画面のSMTPユーザー/パスワードを設定してください）".to_string()
        })?;

        Ok(Self { host, port, user, password })
    }

    /// このメールボックスのチェックポイント識別子（= ユーザー自身）
    pub fn mailbox_id(&self) -> &str {
        &self.user
    }
}

/// IMAP接続してINBOXを読み取り専用(EXAMINE/PEEK)で開く
///
/// EXAMINEは既読フラグを変更しない。「未読/既読に依存しない」設計の根幹。
///
/// この関数はブロッキング（同期）I/Oを行う。async関数から直接呼ぶとTokioのワーカースレッドを
/// 占有してしまうため、必ず `tokio::task::spawn_blocking` の中から呼び出すこと。
///
/// 接続・読み取り双方に `IMAP_TIMEOUT`（30秒）を設定しており、IMAPサーバーが応答しない場合は
/// ハングせず `Err` を返す（2026-08-07、応答なしでスレッドがブロックし続けたことによる
/// ダッシュボードAPIの504タイムアウト障害の再発防止対応）。
pub fn connect_and_examine(config: &ImapConfig) -> Result<ImapSession, String> {
    let addr = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|e| format!("IMAPホスト名前解決エラー: {e}"))?
        .next()
        .ok_or_else(|| "IMAPホストのアドレス解決結果が空です".to_string())?;

    let tcp = TcpStream::connect_timeout(&addr, IMAP_TIMEOUT)
        .map_err(|e| format!("IMAP接続タイムアウト/エラー: {e}"))?;
    tcp.set_read_timeout(Some(IMAP_TIMEOUT))
        .map_err(|e| format!("IMAP読み取りタイムアウト設定エラー: {e}"))?;
    tcp.set_write_timeout(Some(IMAP_TIMEOUT))
        .map_err(|e| format!("IMAP書き込みタイムアウト設定エラー: {e}"))?;

    let tls = native_tls::TlsConnector::new().map_err(|e| format!("TLS初期化エラー: {e}"))?;
    let tls_stream = tls
        .connect(&config.host, tcp)
        .map_err(|e| format!("TLSハンドシェイクエラー: {e}"))?;

    let mut client = imap::Client::new(tls_stream);
    client
        .read_greeting()
        .map_err(|e| format!("IMAP接続エラー(greeting): {e}"))?;

    let mut session = client
        .login(&config.user, &config.password)
        .map_err(|(e, _)| format!("IMAPログインエラー: {e}"))?;
    session
        .examine("INBOX")
        .map_err(|e| format!("INBOX EXAMINE エラー: {e}"))?;
    Ok(session)
}

/// メール1件分の生バイト（ヘッダー・本文）。`Fetch`（内部で借用を持つZeroCopy構造）を
/// spawn_blockingの外へ安全に持ち出せるよう、所有権のあるバイト列にコピーしたもの。
pub struct RawMessage {
    pub header: Vec<u8>,
    pub body: Vec<u8>,
}

/// IMAP接続 → SINCE検索 → ヘッダー/本文取得 → ログアウト、をまとめて1回のブロッキング処理で行う。
///
/// Phase1(メール監視)から使う。この関数全体を `tokio::task::spawn_blocking` の中で呼び出すことで、
/// IMAP通信中もTokioのワーカースレッドが他の非同期リクエスト（ダッシュボードAPI等）の処理を
/// 継続できるようにする。
pub fn fetch_since_blocking(config: &ImapConfig, since_str: &str) -> Result<Vec<RawMessage>, String> {
    let mut session = connect_and_examine(config)?;

    let search_result = session
        .search(format!("SINCE {since_str}"))
        .map_err(|e| format!("メール検索エラー: {e}"))?;

    if search_result.is_empty() {
        let _ = session.logout();
        return Ok(Vec::new());
    }

    let uid_str: Vec<String> = search_result.iter().map(|id| id.to_string()).collect();
    let messages = session
        .fetch(uid_str.join(","), "(BODY.PEEK[HEADER] BODY.PEEK[TEXT] FLAGS)")
        .map_err(|e| format!("メール取得エラー: {e}"))?;

    let raw_messages: Vec<RawMessage> = messages
        .iter()
        .filter_map(|msg| {
            let header = msg.header()?.to_vec();
            let body = msg.text().unwrap_or(b"").to_vec();
            Some(RawMessage { header, body })
        })
        .collect();

    let _ = session.logout();
    Ok(raw_messages)
}

/// 添付ファイル系メールの再取得をまとめて1回のブロッキング処理で行う（Phase2用）。
///
/// `message_ids` の各Message-IDについてIMAP検索・本文取得を順に行い、結果を
/// `message_ids` と同じ順序の `Vec` で返す（呼び出し元は `pending.iter().zip(...)` で対応付ける）。
/// この関数全体を `tokio::task::spawn_blocking` の中で呼び出すこと。
pub fn fetch_attachments_blocking(
    config: &ImapConfig,
    message_ids: &[String],
) -> Result<Vec<Result<Attachment, String>>, String> {
    let mut session = connect_and_examine(config)?;

    let results = message_ids
        .iter()
        .map(|message_id| fetch_one_attachment_blocking(&mut session, message_id))
        .collect();

    let _ = session.logout();
    Ok(results)
}

/// 指定Message-IDのメールをIMAPで検索し、最初の添付ファイルの実体を取得する（Phase2用の内部処理）。
fn fetch_one_attachment_blocking(session: &mut ImapSession, message_id: &str) -> Result<Attachment, String> {
    let search_result = session
        .search(format!("HEADER Message-ID \"{message_id}\""))
        .map_err(|e| format!("メール検索エラー: {e}"))?;

    let uid = search_result
        .iter()
        .next()
        .ok_or_else(|| "IMAP上でメールが見つかりません（削除された可能性）".to_string())?;

    let messages = session
        .fetch(uid.to_string(), "(BODY.PEEK[])")
        .map_err(|e| format!("メール本文取得エラー: {e}"))?;

    let msg = messages
        .iter()
        .next()
        .ok_or_else(|| "メール取得結果が空です".to_string())?;

    let raw_bytes = msg.body().ok_or_else(|| "メール本文が空です".to_string())?;
    let mut attachments = extract_attachments(raw_bytes);

    if attachments.is_empty() {
        return Err("添付ファイルが見つかりませんでした".to_string());
    }

    // 最初の添付を採用する（複数添付があるメールは稀なため、当面は1メール1添付を前提とする）
    Ok(attachments.remove(0))
}

/// ヘッダーから指定フィールドの値を取得
pub fn get_header_value(headers: &[MailHeader], name: &str) -> Option<String> {
    headers
        .iter()
        .find(|h| h.get_key().eq_ignore_ascii_case(name))
        .map(|h| h.get_value())
}

/// From ヘッダーから名前とメールアドレスを抽出（"名前 <email>" → ("名前","email")）
pub fn parse_from_header(from_raw: &str) -> (String, String) {
    let decoded = decode_mime_string(from_raw);

    if let Some(lt) = decoded.find('<') {
        if let Some(gt) = decoded.find('>') {
            let name = decoded[..lt].trim().trim_matches('"').to_string();
            let email = decoded[lt + 1..gt].trim().to_string();
            return (name, email);
        }
    }

    let addr = decoded.trim().trim_matches(|c| c == '<' || c == '>').to_string();
    (String::new(), addr)
}

/// MIMEエンコード文字列(=?UTF-8?B?...?= 等)をデコード
pub fn decode_mime_string(s: &str) -> String {
    let mut result = s.to_string();

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
        break;
    }

    result.trim().to_string()
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

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
            result.push(b' ');
        } else {
            result.push(bytes[i]);
        }
        i += 1;
    }
    Some(result)
}

fn decode_charset(bytes: &[u8], _charset: &str) -> String {
    // ISO-2022-JP/Shift_JIS等も含め lossy 変換で統一（既存実装踏襲）
    String::from_utf8_lossy(bytes).to_string()
}

/// メール本文（text/plain優先、無ければtext/htmlをタグ除去して使用。最大5000文字）を取得
///
/// 旧実装は`parsed.subparts`を1階層しか見ておらず、multipart/mixed > multipart/alternative >
/// text/plain のようにtext/plainが2階層以上ネストしている場合や、HTMLのみ（text/plain無し）の
/// メールで本文が空になる不具合があった（2026-07-14、受信メール画面での実地確認で発覚）。
pub fn extract_body_text(body_bytes: &[u8]) -> String {
    if let Ok(parsed) = mailparse::parse_mail(body_bytes) {
        if let Some(text) = find_text_part(&parsed, "text/plain") {
            return text.chars().take(5000).collect();
        }
        if let Some(html) = find_text_part(&parsed, "text/html") {
            return strip_html_tags(&html).chars().take(5000).collect();
        }
        if let Ok(body) = parsed.get_body() {
            if !body.trim().is_empty() {
                return body.chars().take(5000).collect();
            }
        }
    }
    String::from_utf8_lossy(body_bytes).chars().take(5000).collect()
}

/// 指定MIMEタイプのパートを再帰的に探す（最初に見つかった非空の本文を返す）
fn find_text_part(part: &mailparse::ParsedMail<'_>, mimetype: &str) -> Option<String> {
    if part.ctype.mimetype.eq_ignore_ascii_case(mimetype) {
        if let Ok(body) = part.get_body() {
            if !body.trim().is_empty() {
                return Some(body);
            }
        }
    }
    for sub in &part.subparts {
        if let Some(found) = find_text_part(sub, mimetype) {
            return Some(found);
        }
    }
    None
}

/// 簡易HTMLタグ除去（プレビュー表示用の簡易変換。厳密なHTMLパースは行わない）
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }
    result
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// 添付ファイル1件分（Phase2でダウンロードする実体）
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// メールから添付ファイルの実バイナリを抽出する（名前のみだったPhase1旧実装からの拡張）
pub fn extract_attachments(raw_bytes: &[u8]) -> Vec<Attachment> {
    let mut attachments = Vec::new();
    if let Ok(parsed) = mailparse::parse_mail(raw_bytes) {
        collect_attachments(&parsed, &mut attachments);
    }
    attachments
}

fn collect_attachments(part: &mailparse::ParsedMail<'_>, out: &mut Vec<Attachment>) {
    let disposition = &part.ctype.params;
    let filename = disposition
        .get("name")
        .cloned()
        .or_else(|| {
            part.headers.iter()
                .find(|h| h.get_key().eq_ignore_ascii_case("Content-Disposition"))
                .and_then(|h| extract_filename_from_header(&h.get_value()))
        });

    if let Some(name) = filename {
        if let Ok(body) = part.get_body_raw() {
            out.push(Attachment {
                filename: decode_mime_string(&name),
                content_type: part.ctype.mimetype.clone(),
                bytes: body,
            });
        }
    }

    for sub in &part.subparts {
        collect_attachments(sub, out);
    }
}

/// 添付ファイル名だけを抽出する（Phase1の分類用、バイナリは取得しない軽量版）
pub fn extract_attachment_filenames(raw_bytes: &[u8]) -> Vec<String> {
    extract_attachments(raw_bytes).into_iter().map(|a| a.filename).collect()
}

fn extract_filename_from_header(header_value: &str) -> Option<String> {
    for part in header_value.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix("filename=") {
            return Some(rest.trim_matches('"').to_string());
        }
        if let Some(rest) = trimmed.strip_prefix("filename*=") {
            if let Some(value_start) = rest.find("''") {
                return Some(urlish_decode(&rest[value_start + 2..]));
            }
        }
    }
    None
}

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

/// メールの Date ヘッダーをパース
pub fn parse_email_date(date_str: &str) -> chrono::DateTime<chrono::Utc> {
    match mailparse::dateparse(date_str) {
        Ok(timestamp) => chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(chrono::Utc::now),
        Err(_) => chrono::Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_from_header_extracts_name_and_email() {
        let (name, email) = parse_from_header("山田太郎 <yamada@example.com>");
        assert_eq!(name, "山田太郎");
        assert_eq!(email, "yamada@example.com");
    }

    #[test]
    fn parse_from_header_email_only() {
        let (name, email) = parse_from_header("yamada@example.com");
        assert_eq!(name, "");
        assert_eq!(email, "yamada@example.com");
    }

    #[test]
    fn decode_mime_string_handles_base64_utf8() {
        // "請求書" を UTF-8 Base64 でエンコードしたもの
        let encoded = "=?UTF-8?B?6KuL5rGC5pu4?=";
        assert_eq!(decode_mime_string(encoded), "請求書");
    }

    #[test]
    fn extract_body_text_decodes_base64_text_plain_nested_one_level() {
        // Outlook形式: multipart/mixed > (boundary) > text/plain(base64) という
        // 1階層ネストの構造。旧実装はparsed.subparts直下しか見ておらず、この構造では
        // 本文が空になっていた（2026-07-14、実メールで発覚した不具合の再現ケース）。
        let body_b64 = base64_encode("こんにちは。テスト本文です。");
        let raw = format!(
            "Content-Type: multipart/mixed; boundary=\"outer\"\r\n\r\n\
             --outer\r\n\
             Content-Type: text/plain; charset=\"utf-8\"\r\n\
             Content-Transfer-Encoding: base64\r\n\r\n\
             {body_b64}\r\n\
             --outer--\r\n"
        );
        let result = extract_body_text(raw.as_bytes());
        assert_eq!(result, "こんにちは。テスト本文です。");
    }

    #[test]
    fn extract_body_text_falls_back_to_html_when_no_plain_part() {
        let raw = "Content-Type: text/html; charset=\"utf-8\"\r\n\r\n\
                   <html><body><p>HTMLのみの本文</p></body></html>\r\n";
        let result = extract_body_text(raw.as_bytes());
        assert_eq!(result, "HTMLのみの本文");
    }

    /// テスト用の最小限base64エンコーダ（外部crateのbase64を使わず標準アルファベットのみ実装）
    fn base64_encode(input: &str) -> String {
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let bytes = input.as_bytes();
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0];
            let b1 = *chunk.get(1).unwrap_or(&0);
            let b2 = *chunk.get(2).unwrap_or(&0);
            out.push(CHARS[(b0 >> 2) as usize] as char);
            out.push(CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
            out.push(if chunk.len() > 1 { CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char } else { '=' });
            out.push(if chunk.len() > 2 { CHARS[(b2 & 0x3f) as usize] as char } else { '=' });
        }
        out
    }
}
