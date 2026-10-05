//! custom/registry_none.rs — 取引先ごとのカスタマイズが無いとき（公開版）の組み込み。すべて「何もしない」。
//!
//! 社内でも `cargo test --no-default-features` でこのファイルを使い、差し込み口の形とずれないことを確かめる。

use sqlx::PgPool;

use super::{EdiDocument, EdiFetchSummary, EdiNoticeSender, OrderPdfFormat};
use crate::domain::models::mail_pipeline::PhaseError;

/// 取引先ごとの注文書 PDF の様式は無い
pub fn order_pdf_formats() -> Vec<&'static OrderPdfFormat> {
    Vec::new()
}

/// 勤務表 PDF の様式は無い
pub fn parse_timesheet_pdf(
    file_bytes: &[u8],
    original_filename: &str,
) -> crate::domain::services::excel_parser::TimesheetParseResult {
    let _ = (file_bytes, original_filename);
    crate::domain::services::excel_parser::error_result_for_pdf(
        "この勤務表PDFの様式には対応していません（PDFの勤務表は、取引先ごとのカスタマイズで読み取ります）",
    )
}

/// 取引先マスタ「EDI方式」の選択肢
pub const EDI_OPTIONS: &[(&str, &str)] = &[("", "なし"), ("EMAIL", "メール")];

/// 取引先 EDI の通知メールの送り元は無い
pub async fn edi_notice_sender(pool: &PgPool, from_email: &str) -> Option<EdiNoticeSender> {
    let _ = (pool, from_email);
    None
}

/// 取引先 EDI の通知メールは処理しない
pub async fn fetch_edi_notice_documents(pool: &PgPool) -> EdiFetchSummary {
    let _ = pool;
    EdiFetchSummary::default()
}

/// 取引先 EDI の書類は読めない
pub fn interpret_edi_document(
    message_id: &str,
    client_id: Option<i64>,
    parsed_data: Option<&serde_json::Value>,
) -> Result<EdiDocument, PhaseError> {
    let _ = (client_id, parsed_data);
    Err(PhaseError::Permanent(format!(
        "取引先EDIの書類ですが、対応するカスタマイズが組み込まれていません（message_id={message_id}）"
    )))
}
