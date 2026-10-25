/// domain/services/issued_document.rs — 発行した書類の PDF を、送った瞬間のまま残す(DEMO-000214)
///
/// 設計: docs/spec/アプリ概要設計書.md §6.4・アプリ方式設計書.md §8.3・§10.5。
/// - 送った瞬間に PDF を1回だけ作り、DB に残す(正本。D6)。相手がリンクから開いたときも、これを見せる。
/// - Drive への保存(写し)は、送信と切り離す。すぐに1回試し、失敗したら 15 分ごとのジョブがやり直す。
///   3回続けて失敗したら、異常の通知(ops_alert)で知らせる。
/// - 送り直しで内容が変わったら新しい版にする(Drive では `_v2` … の別ファイル。D2)。内容が同じなら版を増やさない。
/// - 保存先: 根(`integrations.toml` の issued_pdf_archive)の下の
///   `パートナー管理/<名>/{注文書（当社発行）,支払通知書}`・`クライアント管理/<名>/請求書`。無いフォルダは作る。
/// - ファイル名: `<書類番号>_<対象年月>.pdf`(D1)。承諾は `<注文書ID>_<対象年月>_承諾.pdf`。

use chrono::NaiveDate;
use sqlx::PgPool;

use crate::infrastructure::repositories::issued_document_repo::{self as repo, NewIssuedDocument};

/// 3回続けて失敗したら知らせる
const ALERT_AFTER_ATTEMPTS: i32 = 3;
/// 1回のジョブで扱う件数
const BATCH: i64 = 20;

/// 残す書類の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocType {
    /// 注文書(当社発行)
    PurchaseOrder,
    /// 支払通知書
    PaymentNotice,
    /// 請求書
    Invoice,
    /// 注文請書(パートナーの承諾)
    Acceptance,
    /// 代理請求書(支払通知書をパートナーが承諾した時点の、パートナー名義の請求書。DEMO-000087)
    PartnerInvoice,
}

impl DocType {
    pub fn code(self) -> &'static str {
        match self {
            DocType::PurchaseOrder => "purchase_order",
            DocType::PaymentNotice => "payment_notice",
            DocType::Invoice => "invoice",
            DocType::Acceptance => "acceptance",
            DocType::PartnerInvoice => "partner_invoice",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "purchase_order" => Some(DocType::PurchaseOrder),
            "payment_notice" => Some(DocType::PaymentNotice),
            "invoice" => Some(DocType::Invoice),
            "acceptance" => Some(DocType::Acceptance),
            "partner_invoice" => Some(DocType::PartnerInvoice),
            _ => None,
        }
    }

    pub fn party_kind(self) -> &'static str {
        match self {
            DocType::Invoice => "client",
            _ => "partner",
        }
    }

    fn label_ja(self) -> &'static str {
        match self {
            DocType::PurchaseOrder => "注文書",
            DocType::PaymentNotice => "支払通知書",
            DocType::Invoice => "請求書",
            DocType::Acceptance => "注文請書",
            DocType::PartnerInvoice => "代理請求書",
        }
    }

    /// 取引先のフォルダの下の、書類の種類のフォルダ(括弧は全角。決定事項どおり)
    pub fn folder_label(self) -> &'static str {
        match self {
            DocType::PurchaseOrder | DocType::Acceptance => "注文書（当社発行）",
            DocType::PaymentNotice | DocType::PartnerInvoice => "支払通知書",
            DocType::Invoice => "請求書",
        }
    }
}

fn management_label(party_kind: &str) -> &'static str {
    if party_kind == "client" {
        "クライアント管理"
    } else {
        "パートナー管理"
    }
}

/// フォルダ名に使えない・紛らわしい文字を置き換える(取引先名は人が入力したものなので)
pub fn sanitize_folder_name(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' => '／',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "（取引先名なし）".to_string()
    } else {
        s
    }
}

/// ファイル名: `<書類番号>_<対象年月>.pdf`。対象年月が無ければ番号だけ
pub fn file_name(doc_type: DocType, doc_ref: &str, target_month: Option<NaiveDate>) -> String {
    let base = sanitize_folder_name(doc_ref).replace(' ', "_");
    let month = target_month.map(|d| format!("_{}", d.format("%Y-%m"))).unwrap_or_default();
    let suffix = match doc_type {
        DocType::Acceptance => "_承諾",
        DocType::PartnerInvoice => "_代理請求書",
        _ => "",
    };
    format!("{base}{month}{suffix}.pdf")
}

/// 送った書類1件
pub struct Issued<'a> {
    pub doc_type: DocType,
    pub doc_ref: &'a str,
    pub party_name: &'a str,
    pub target_month: Option<NaiveDate>,
    pub amount: Option<i64>,
    pub pdf: &'a [u8],
}

/// 送った書類を残し、Drive への保存を裏で1回試す。
/// 失敗しても送信は成功のまま(呼び出し元はエラーを返さない)。残せなかったときは error ログと異常の通知を出す。
pub async fn record_sent(pool: &PgPool, d: Issued<'_>) {
    match record_sent_inner(pool, &d).await {
        Ok(Some(id)) => {
            let pool = pool.clone();
            tokio::spawn(async move {
                archive_one_by_id(&pool, id).await;
            });
        }
        Ok(None) => {}
        Err(e) => {
            let msg = format!(
                "{} {} を送りましたが、送った PDF を残せませんでした(相手がリンクを開くと、その時点の内容で作り直した PDF が出ます): {e}",
                d.doc_type.label_ja(),
                d.doc_ref
            );
            tracing::error!("[発行した書類] {msg}");
            let incident = crate::domain::services::chat_notifier::Incident::new(
                integration_core::Level::High,
                format!("{} {} を送りましたが、送った PDF を残せませんでした", d.doc_type.label_ja(), d.doc_ref),
                "送った時点の PDF が残りません。相手がリンクを開くと、その時点の内容で作り直した PDF が出ます",
                format!("DB の状態を確認してください。理由: {e}"),
            );
            crate::domain::services::chat_notifier::post(&incident).await;
        }
    }
}

/// 「前の版と同じ内容か」を比べるための形にする。
/// PDF を作る部品(ReportLab)は、作るたびに作成日時(/CreationDate・/ModDate)と識別子(/ID)を埋め込むので、
/// 中身が同じでもバイト列は毎回違う。比べるときだけ、これらの値を消して比べる(保存する PDF は変えない)。
pub fn comparable_form(pdf: &[u8]) -> Vec<u8> {
    fn blank_after(src: &[u8], key: &[u8], open: u8, close: u8) -> Vec<u8> {
        let mut res = Vec::with_capacity(src.len());
        let mut i = 0;
        while i < src.len() {
            if src[i..].starts_with(key) {
                res.extend_from_slice(key);
                i += key.len();
                // 空白を飛ばして、開きの記号から閉じの記号までを空にする
                let mut j = i;
                while j < src.len() && src[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < src.len() && src[j] == open {
                    if let Some(end) = src[j + 1..].iter().position(|&b| b == close) {
                        res.push(open);
                        res.push(close);
                        i = j + 1 + end + 1;
                        continue;
                    }
                }
                continue;
            }
            res.push(src[i]);
            i += 1;
        }
        res
    }
    let a = blank_after(pdf, b"/CreationDate", b'(', b')');
    let b = blank_after(&a, b"/ModDate", b'(', b')');
    blank_after(&b, b"/ID", b'[', b']')
}

/// 残した記録の id(内容が最新の版と同じで、新しく残さなかったときは None)
async fn record_sent_inner(pool: &PgPool, d: &Issued<'_>) -> anyhow::Result<Option<i64>> {
    if d.pdf.is_empty() {
        anyhow::bail!("PDF が空です");
    }
    let code = d.doc_type.code();
    if let Some(latest) = repo::latest_meta(pool, code, d.doc_ref).await? {
        let same = latest.sha256 == repo::sha256_hex(d.pdf)
            || repo::load_content(pool, latest.id).await.map(|prev| comparable_form(&prev) == comparable_form(d.pdf)).unwrap_or(false);
        if same {
            tracing::info!("[発行した書類] {} {} は前回と同じ内容のため、版を増やしません(版 {})", d.doc_type.label_ja(), d.doc_ref, latest.version);
            return Ok(None);
        }
    }
    let name = file_name(d.doc_type, d.doc_ref, d.target_month);
    let party = sanitize_folder_name(d.party_name);
    let (id, version) = repo::insert_new_version(
        pool,
        &NewIssuedDocument {
            doc_type: code,
            doc_ref: d.doc_ref,
            party_kind: d.doc_type.party_kind(),
            party_name: &party,
            target_month: d.target_month,
            amount: d.amount,
            file_name: &name,
            content: d.pdf,
        },
    )
    .await?;
    tracing::info!("[発行した書類] {} {} を残しました(版 {version}、id {id})", d.doc_type.label_ja(), d.doc_ref);
    Ok(Some(id))
}

/// 送った PDF(最新の版)。無ければ None(古い書類など。呼び出し元は従来どおり作り直す)
pub async fn frozen_pdf(pool: &PgPool, doc_type: DocType, doc_ref: &str) -> Option<Vec<u8>> {
    let meta = match repo::latest_meta(pool, doc_type.code(), doc_ref).await {
        Ok(Some(m)) => m,
        Ok(None) => return None,
        Err(e) => {
            tracing::error!("[発行した書類] {} {} の記録を読めません: {e}", doc_type.label_ja(), doc_ref);
            return None;
        }
    };
    match repo::load_content(pool, meta.id).await {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            tracing::error!("[発行した書類] {} {} の本体を読めません: {e}", doc_type.label_ja(), doc_ref);
            None
        }
    }
}

/// `Content-Disposition: inline` の値。日本語のファイル名(承諾など)も崩れないよう、RFC 6266/5987 の形にする
pub fn content_disposition_inline(name: &str) -> String {
    let ascii: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' })
        .collect();
    let mut encoded = String::new();
    for b in name.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(b) {
            encoded.push(*b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    format!("inline; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

async fn archive_one_by_id(pool: &PgPool, id: i64) {
    match repo::list_pending(pool, BATCH).await {
        Ok(rows) => {
            if let Some(m) = rows.into_iter().find(|m| m.id == id) {
                archive_one(pool, &m).await;
            }
        }
        Err(e) => tracing::error!("[発行した書類] 保存待ちの記録を読めません: {e}"),
    }
}

/// Drive へ1件保存する
async fn archive_one(pool: &PgPool, m: &repo::IssuedDocumentMeta) {
    let store = match crate::infrastructure::integrations::issued_pdf_store() {
        Ok(s) => s,
        Err(why) => {
            // 未設定は起動時・毎朝の点検で知らせる。ここでは失敗の回数を数えない(設定されたら、たまった分を保存する)
            tracing::info!("[発行した書類] 保存先が未設定のため、Drive への保存を待ちます(id {}: {why})", m.id);
            return;
        }
    };
    let Some(doc_type) = DocType::from_code(&m.doc_type) else {
        tracing::error!("[発行した書類] 知らない書類の種類です(id {}: {})", m.id, m.doc_type);
        return;
    };
    let bytes = match repo::load_content(pool, m.id).await {
        Ok(b) => b,
        Err(e) => {
            fail(pool, m, &format!("本体を読めません: {e}")).await;
            return;
        }
    };
    let folder = [management_label(&m.party_kind), m.party_name.as_str(), doc_type.folder_label()];
    match store.put_versioned(&folder, &m.file_name, &bytes, "application/pdf").await {
        Ok(f) => {
            if let Err(e) = repo::mark_stored(pool, m.id, &f.id, &f.name, f.url.as_deref()).await {
                tracing::error!("[発行した書類] Drive へ保存しましたが、記録を更新できません(id {}): {e}", m.id);
            } else {
                tracing::info!("[発行した書類] Drive へ保存しました: {}/{}/{}/{}", folder[0], folder[1], folder[2], f.name);
            }
        }
        Err(e) => fail(pool, m, &e.to_string()).await,
    }
}

async fn fail(pool: &PgPool, m: &repo::IssuedDocumentMeta, error: &str) {
    let attempts = match repo::mark_attempt_failed(pool, m.id, error).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("[発行した書類] 失敗の記録もできません(id {}): {e}", m.id);
            return;
        }
    };
    tracing::warn!("[発行した書類] Drive への保存に失敗しました(id {}、{attempts}回目): {error}", m.id);
    if attempts == ALERT_AFTER_ATTEMPTS {
        let label = DocType::from_code(&m.doc_type).map(|d| d.label_ja()).unwrap_or("書類");
        let incident = crate::domain::services::chat_notifier::Incident::new(
            integration_core::Level::Warning,
            format!("発行した{label} {}(版 {})の PDF を、Drive へ{attempts}回続けて保存できませんでした", m.doc_ref, m.version),
            "PDF は Sophia の DB に残っています(正本)。Drive の写しだけが未保存で、15 分ごとにやり直します",
            format!("続く場合は、設定 > 外部連携 で保存先を点検してください。理由: {error}"),
        );
        crate::domain::services::chat_notifier::post(&incident).await;
    }
}

/// 15 分ごとのジョブ: 保存待ちの書類を Drive へ保存する
pub async fn archive_pending(pool: &PgPool) {
    let rows = match repo::list_pending(pool, BATCH).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[発行した書類] 保存待ちの記録を読めません: {e}");
            return;
        }
    };
    for m in &rows {
        archive_one(pool, m).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_follow_decision() {
        let m = NaiveDate::from_ymd_opt(2026, 10, 1);
        assert_eq!(file_name(DocType::PurchaseOrder, "PO-202610-001", m), "PO-202610-001_2026-10.pdf");
        assert_eq!(file_name(DocType::Acceptance, "PO-202610-001", m), "PO-202610-001_2026-10_承諾.pdf");
        assert_eq!(file_name(DocType::Invoice, "SI001", None), "SI001.pdf");
        assert_eq!(file_name(DocType::PartnerInvoice, "PN-1", m), "PN-1_2026-10_代理請求書.pdf");
    }

    #[test]
    fn folders_use_fullwidth_parens() {
        assert_eq!(DocType::PurchaseOrder.folder_label(), "注文書（当社発行）");
        assert_eq!(DocType::Acceptance.folder_label(), "注文書（当社発行）");
        assert_eq!(management_label(DocType::Invoice.party_kind()), "クライアント管理");
        assert_eq!(management_label(DocType::PaymentNotice.party_kind()), "パートナー管理");
    }

    #[test]
    fn sanitize_replaces_slash_and_blank() {
        assert_eq!(sanitize_folder_name(" A/B社 "), "A／B社");
        assert_eq!(sanitize_folder_name("  "), "（取引先名なし）");
        assert_eq!(sanitize_folder_name("a\tb"), "a b");
    }

    #[test]
    fn content_disposition_handles_japanese() {
        assert_eq!(
            content_disposition_inline("PO-1_2026-10.pdf"),
            "inline; filename=\"PO-1_2026-10.pdf\"; filename*=UTF-8''PO-1_2026-10.pdf"
        );
        let v = content_disposition_inline("PO-1_承諾.pdf");
        assert!(v.is_ascii());
        assert!(v.contains("filename*=UTF-8''PO-1_%E6%89%BF%E8%AB%BE.pdf"));
    }

    #[test]
    fn comparable_form_ignores_dates_and_id() {
        let a = b"%PDF-1.4 /Author (x) /CreationDate (D:20261011132229+00'00') /ModDate (D:20261011132229+00'00') trailer << /ID \n[<aa><bb>] >> body".to_vec();
        let b = b"%PDF-1.4 /Author (x) /CreationDate (D:20261011132434+00'00') /ModDate (D:20261011132434+00'00') trailer << /ID \n[<cc><dd>] >> body".to_vec();
        assert_ne!(a, b);
        assert_eq!(comparable_form(&a), comparable_form(&b));
        let c = b"%PDF-1.4 /Author (x) /CreationDate (D:1) /ModDate (D:1) trailer << /ID [<cc><dd>] >> other".to_vec();
        assert_ne!(comparable_form(&a), comparable_form(&c), "中身が違えば違う");
    }

    #[test]
    fn codes_round_trip() {
        for d in [DocType::PurchaseOrder, DocType::PaymentNotice, DocType::Invoice, DocType::Acceptance, DocType::PartnerInvoice] {
            assert_eq!(DocType::from_code(d.code()), Some(d));
        }
    }
}
