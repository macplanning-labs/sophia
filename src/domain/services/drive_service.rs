/// domain/services/drive_service.rs — Google Drive連携サービス
///
/// Django版 core/domain/services/drive_service.py の移植。
/// サービスアカウント認証でGoogle Driveにドキュメントをアップロードする。
///
/// フォルダ構成:
///     Sophia (ルート)
///     ├── パートナー管理/
///     │   └── {会社名}/
///     │       ├── 契約書/
///     │       ├── 注文書/
///     │       └── 稼働報告書/
///     └── クライアント管理/
///         └── {会社名}/
///             ├── 注文書/
///             ├── 稼働報告書/
///             └── 請求書/

use drive_core::DriveClient;
use tracing::info;

type Result<T> = drive_core::Result<T>;

// ── ドキュメント種別ラベル ──

const DOC_TYPE_LABELS: &[(&str, &str)] = &[
    ("contract", "契約書"),
    ("order", "注文書"),
    ("work_report", "稼働報告書"),
    ("invoice", "請求書"),
    ("payment", "支払通知書"),
];

fn doc_type_label(doc_type: &str) -> &str {
    DOC_TYPE_LABELS.iter()
        .find(|(k, _)| *k == doc_type)
        .map(|(_, v)| *v)
        .unwrap_or(doc_type)
}

// ============================================================
// 統一ドキュメントアップロードAPI
// ============================================================

/// 統一ドキュメントアップロードAPI
///
/// management_type: "partner" or "client"
/// company_name: パートナー名 or クライアント名
/// doc_type: "contract", "order", "work_report", "invoice", "payment"
/// filename: ファイル名
/// file_bytes: ファイルバイトデータ
///
/// Returns: (file_id, web_link) or エラー
pub async fn upload_document(
    management_type: &str,
    company_name: &str,
    doc_type: &str,
    filename: &str,
    file_bytes: &[u8],
) -> Result<(String, String)> {
    let Some(client) = DriveClient::from_env() else {
        return Ok((String::new(), String::new()));
    };

    let mgmt_label = if management_type == "partner" { "パートナー管理" } else { "クライアント管理" };
    let doc_label = doc_type_label(doc_type);
    let folder_path = [mgmt_label, company_name, doc_label];

    let uploaded = client
        .upload_file(&folder_path, filename, file_bytes, "application/pdf")
        .await?;

    info!(
        "[Google Drive] アップロード成功: {}/{}/{}/{} → {}",
        mgmt_label, company_name, doc_label, filename, uploaded.file_id
    );

    Ok((uploaded.file_id, uploaded.web_view_link))
}

// ============================================================
// 公開API — Sophia固有ラッパー
// ============================================================

/// 注文書PDFをGoogle Driveにアップロードする
pub async fn upload_order_pdf(partner_name: &str, order_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document(
        "partner",
        partner_name,
        "order",
        &format!("order_{}.pdf", order_id),
        pdf_bytes,
    ).await
}

/// 支払通知書PDFをGoogle Driveにアップロードする
pub async fn upload_payment_notice_pdf(partner_name: &str, notice_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document(
        "partner",
        partner_name,
        "payment",
        &format!("payment_{}.pdf", notice_id),
        pdf_bytes,
    ).await
}

/// 請求書PDFをGoogle Driveにアップロードする
pub async fn upload_invoice_pdf(client_name: &str, invoice_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document(
        "client",
        client_name,
        "invoice",
        &format!("invoice_{}.pdf", invoice_id),
        pdf_bytes,
    ).await
}

/// 稼働報告書をGoogle Driveにアップロードする
pub async fn upload_work_report(client_name: &str, filename: &str, file_bytes: &[u8]) -> Result<(String, String)> {
    upload_document(
        "client",
        client_name,
        "work_report",
        filename,
        file_bytes,
    ).await
}

/// 基本契約書PDFをGoogle Driveにアップロードする
pub async fn upload_contract_pdf(partner_name: &str, date_str: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document(
        "partner",
        partner_name,
        "contract",
        &format!("契約書_{}_{}.pdf", partner_name, date_str),
        pdf_bytes,
    ).await
}

/// DriveファイルIDからURLを生成する
pub fn get_drive_file_url(file_id: &str) -> String {
    if file_id.is_empty() {
        String::new()
    } else {
        format!("https://drive.google.com/file/d/{}/view", file_id)
    }
}
