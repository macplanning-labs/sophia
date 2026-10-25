/// infrastructure/drive_service.rs — Google Drive連携サービス
///
/// Django版 core/domain/services/drive_service.py (362行) から移植。
/// サービスアカウント認証でGoogle Driveにドキュメントをアップロードする。
///
/// フォルダ構成:
///     Sophia (ルート)
///     ├── パートナー管理/
///     │   └── {会社名}/
///     │       ├── 契約書/
///     │       ├── 注文書（当社発行）/
///     │       ├── 支払通知書/
///     │       └── 稼働報告書/
///     └── クライアント管理/
///         └── {会社名}/
///             ├── 注文書（クライアント発行）/
///             ├── 稼働報告書/
///             └── 請求書/
///
/// 発行した書類(送った瞬間の PDF)の保存は domain/services/issued_document.rs(DEMO-000214)。

use anyhow::Result;
use tracing::info;

// ── ドキュメント種別ラベル ──

/// 書類の種類のフォルダ名(決定事項 2026-10-11。括弧は全角)
/// 注文書は、クライアントから受けたものとパートナーへ出したもので分ける。
fn doc_type_label<'a>(management_type: &str, doc_type: &'a str) -> &'a str {
    match doc_type {
        "contract" => "契約書",
        "order" if management_type == "partner" => "注文書（当社発行）",
        "order" => "注文書（クライアント発行）",
        "work_report" => "稼働報告書",
        "invoice" => "請求書",
        "payment" => "支払通知書",
        _ => doc_type,
    }
}

/// サービスアカウント設定（`GOOGLE_DRIVE_CREDENTIALS_FILE`）から指定スコープのアクセストークンを取得する。
/// 未設定ならNone（呼び出し元は機能をスキップする）。`sheets_service.rs`など他のGoogle API連携でも
/// 同じサービスアカウントを使い回すための共通ヘルパー。
pub(crate) async fn get_scoped_access_token(scope: &str) -> Result<Option<String>> {
    let impersonate = std::env::var("GOOGLE_DRIVE_IMPERSONATE_EMAIL").ok();
    google_auth::get_access_token_from_file("GOOGLE_DRIVE_CREDENTIALS_FILE", impersonate.as_deref(), scope).await
        .map_err(|e| anyhow::anyhow!("{}", e))
}

fn guess_mime(filename: &str) -> &'static str {
    if filename.ends_with(".xlsx") || filename.ends_with(".xlsm") {
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    } else if filename.ends_with(".html") {
        "text/html"
    } else {
        "application/pdf"
    }
}

// ============================================================
// 統一ドキュメントアップロードAPI
// ============================================================

/// 統一ドキュメントアップロードAPI
///
/// management_type: "partner" or "client"
/// company_name: パートナー名 or クライアント名
/// doc_type: "contract", "order", "work_report", "invoice", "payment"
///
/// 保存は外部連携の部品（`integrations::file_store`、Google Drive）で行う（DEMO-000213）。
/// 同名ファイルは上書きする（受信メールの添付の再取込で重複させないため。従来どおり）。
/// 保存先が未設定なら何もせず空の (file_id, link) を返す（従来どおり。欠けは起動時の点検で知らせる）。
pub async fn upload_document(
    management_type: &str,
    company_name: &str,
    doc_type: &str,
    filename: &str,
    file_bytes: &[u8],
    mimetype: Option<&str>,
) -> Result<(String, String)> {
    let store = match crate::infrastructure::integrations::file_store() {
        Ok(s) => s,
        Err(why) => {
            info!("[Google Drive] 保存先が未設定のためスキップ（{}）", why);
            return Ok((String::new(), String::new()));
        }
    };

    let mgmt_label = if management_type == "partner" {
        "パートナー管理"
    } else {
        "クライアント管理"
    };
    let folder = [mgmt_label, company_name, doc_type_label(management_type, doc_type)];
    let mime = mimetype.unwrap_or_else(|| guess_mime(filename));

    let stored = store
        .put_overwrite(&folder, filename, file_bytes, mime)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    info!("[Google Drive] 保存しました: {}/{}/{}/{} (ID: {})", folder[0], folder[1], folder[2], stored.name, stored.id);
    let link = stored.url.unwrap_or_else(|| get_drive_file_url(&stored.id));
    Ok((stored.id, link))
}

// ============================================================
// 公開API — Sophia固有ラッパー
// ============================================================

/// 注文書PDFをGoogle Driveにアップロードする
pub async fn upload_order_pdf(partner_name: &str, order_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("partner", partner_name, "order", &format!("order_{}.pdf", order_id), pdf_bytes, None).await
}

/// 支払通知書PDFをGoogle Driveにアップロードする
pub async fn upload_payment_notice_pdf(partner_name: &str, notice_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("partner", partner_name, "payment", &format!("payment_{}.pdf", notice_id), pdf_bytes, None).await
}

/// 請求書PDFをGoogle Driveにアップロードする
pub async fn upload_invoice_pdf(client_name: &str, invoice_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("client", client_name, "invoice", &format!("invoice_{}.pdf", invoice_id), pdf_bytes, None).await
}

/// 稼働報告書をGoogle Driveにアップロードする
pub async fn upload_work_report(client_name: &str, filename: &str, file_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("client", client_name, "work_report", filename, file_bytes, None).await
}

/// 契約書PDFをGoogle Driveにアップロードする
pub async fn upload_contract_pdf(partner_name: &str, signed_at: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    let filename = format!("契約書_{}_{}.pdf", partner_name, signed_at);
    upload_document("partner", partner_name, "contract", &filename, pdf_bytes, None).await
}

/// DriveファイルIDからURLを生成する
pub fn get_drive_file_url(file_id: &str) -> String {
    if file_id.is_empty() {
        String::new()
    } else {
        format!("https://drive.google.com/file/d/{}/view", file_id)
    }
}

#[cfg(test)]
mod tests {
    use super::doc_type_label;

    #[test]
    fn order_folders_split_by_party() {
        assert_eq!(doc_type_label("client", "order"), "注文書（クライアント発行）");
        assert_eq!(doc_type_label("partner", "order"), "注文書（当社発行）");
        assert_eq!(doc_type_label("partner", "work_report"), "稼働報告書");
        assert_eq!(doc_type_label("client", "invoice"), "請求書");
    }
}
