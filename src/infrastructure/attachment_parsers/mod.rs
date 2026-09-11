/// infrastructure/attachment_parsers — 添付ファイル解析ディスパッチ（Phase3から利用）
///
/// PDFバイト列からテキストを抽出し(pdf-extract crate)、書類種別(注文書 / 支払通知書・請求書)に
/// 応じて order_pdf_parser / payment_notice_pdf_parser のいずれかへディスパッチする。
/// Excel・勤務表PDF(稼働報告書)は既存の `domain::services::excel_parser::auto_detect_and_parse`
/// （PDF時は `timesheet_pdf_parser` へディスパッチ）をPhase3が直接呼ぶため、ここでは扱わない。
///
/// 正規表現パースで必須項目(注文番号/単価、または合計金額)が一切取れなかった場合は
/// `PhaseError::Permanent` を返し、呼び出し元(Phase3)が needs_manual_review へ倒す。
/// 将来、正規表現で抽出できなかった書類をAI/OCR(Claude Vision等)へ回すフックを
/// ここに追加する想定（現時点では未実装）。

pub mod order_pdf_parser;
pub mod payment_notice_pdf_parser;

pub use order_pdf_parser::ParsedOrder;
pub use payment_notice_pdf_parser::ParsedPaymentNotice;

use crate::domain::models::mail_pipeline::PhaseError;

/// 添付PDFが表す書類の種別（Phase3が件名/EDI区分から判定して渡す）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// 注文書・発注書
    Order,
    /// 支払通知書・請求書
    PaymentNotice,
}

/// パース結果（書類種別ごとに異なる構造）
pub enum ParsedDocument {
    Order(ParsedOrder),
    PaymentNotice(ParsedPaymentNotice),
}

/// PDFバイト列をテキスト抽出した上で、書類種別に応じたパーサーへディスパッチする
pub fn parse_pdf(kind: DocumentKind, pdf_bytes: &[u8]) -> Result<ParsedDocument, PhaseError> {
    let text = pdf_extract::extract_text_from_mem(pdf_bytes)
        .map_err(|e| PhaseError::Permanent(format!("PDFテキスト抽出失敗: {e}")))?;

    match kind {
        DocumentKind::Order => {
            let parsed = order_pdf_parser::parse(&text);
            if parsed.client_order_number.is_none() && parsed.unit_price.is_none() {
                return Err(PhaseError::Permanent(
                    "注文書PDFから必須項目(注文番号/単価)を抽出できませんでした".to_string(),
                ));
            }
            Ok(ParsedDocument::Order(parsed))
        }
        DocumentKind::PaymentNotice => {
            let parsed = payment_notice_pdf_parser::parse(&text);
            if parsed.total.is_none() {
                return Err(PhaseError::Permanent(
                    "支払通知書/請求書PDFから合計金額を抽出できませんでした".to_string(),
                ));
            }
            Ok(ParsedDocument::PaymentNotice(parsed))
        }
    }
}
