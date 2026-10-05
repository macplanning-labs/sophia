/// infrastructure/attachment_parsers/order_pdf_parser.rs — 注文書PDF自動パーサー
///
/// EDI_MP `billing/services/order_pdf_parser.py`（pdfminer + 正規表現、本番実績あり）を
/// そのままRustへ移植したもの。テキスト抽出自体は呼び出し元(mod.rs)がpdf-extractで行い、
/// この関数は抽出済みテキストを受け取って構造化するだけ。
///
/// 対応形式:
///   - 取引先ごとのカスタマイズが足す形式（`crate::custom::ORDER_PDF_FORMATS`。先に判定する）
///   - 汎用形式（正規表現でベストエフォートパース）
///
/// 文字クラス中の `⽂/⽉/⾦/⾜/⽀` はCJK部首互換文字（U+2E80台）。
/// PDFのフォントサブセット化の都合でpdfminer/pdf-extractがこれらを通常の
/// 「文/月/金/足/支」の代わりに抽出することがあるため、Python版と同様に両方を許容する。

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;
use crate::domain::services::static_regex::compile_static;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParsedOrder {
    pub client_order_number: Option<String>,
    pub order_date: Option<NaiveDate>,
    pub project_name: Option<String>,
    pub work_start: Option<NaiveDate>,
    pub work_end: Option<NaiveDate>,
    pub unit_price: Option<i64>,
    pub time_lower: Option<f64>,
    pub time_upper: Option<f64>,
    pub excess_rate: Option<i64>,
    pub shortage_rate: Option<i64>,
    pub person_name: Option<String>,
    pub payment_terms: Option<String>,
    pub format: &'static str,
}

/// 抽出済みPDFテキストをパースし、フォーマットを自動判定する
pub fn parse(text: &str) -> ParsedOrder {
    let mut result = ParsedOrder::default();

    if let Some(f) = crate::custom::order_pdf_formats().into_iter().find(|f| (f.detect)(text)) {
        result.format = f.name;
        (f.parse)(text, &mut result);
    } else {
        result.format = "generic";
        parse_generic(text, &mut result);
    }

    result
}

pub(crate) fn parse_amount(s: &str) -> Option<i64> {
    s.replace(',', "").parse::<i64>().ok()
}

pub(crate) fn ymd(y: &str, m: &str, d: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
}

fn parse_generic(text: &str, result: &mut ParsedOrder) {
    static ORDER_NO_RE: LazyLock<Regex> = LazyLock::new(|| {
        compile_static(r"(?:注文番号|発注番号|PO番号)[：:\s]*([A-Za-z0-9\-]+)")
    });
    if let Some(c) = ORDER_NO_RE.captures(text) {
        result.client_order_number = Some(c[1].to_string());
    }

    static DATE_RE: LazyLock<Regex> =
        LazyLock::new(|| compile_static(r"(\d{4})[/\-年](\d{1,2})[/\-月](\d{1,2})"));
    if let Some(c) = DATE_RE.captures(text) {
        result.order_date = ymd(&c[1], &c[2], &c[3]);
    }

    static UNIT_PRICE_RE: LazyLock<Regex> =
        LazyLock::new(|| compile_static(r"(?:単価|月額)[：:\s]*[￥¥]?([\d,]+)"));
    if let Some(c) = UNIT_PRICE_RE.captures(text) {
        result.unit_price = parse_amount(&c[1]);
    }

    static TIME_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        compile_static(r"([\d.]+)\s*h?\s*[〜～~ー]\s*([\d.]+)\s*h?")
    });
    if let Some(c) = TIME_RANGE_RE.captures(text) {
        result.time_lower = c[1].parse().ok();
        result.time_upper = c[2].parse().ok();
    }

    static EXCESS_RE: LazyLock<Regex> =
        LazyLock::new(|| compile_static(r"超過[^\d]*[￥¥]?([\d,]+)"));
    if let Some(c) = EXCESS_RE.captures(text) {
        result.excess_rate = parse_amount(&c[1]);
    }

    static SHORTAGE_RE: LazyLock<Regex> =
        LazyLock::new(|| compile_static(r"(?:控除|不足)[^\d]*[￥¥]?([\d,]+)"));
    if let Some(c) = SHORTAGE_RE.captures(text) {
        result.shortage_rate = parse_amount(&c[1]);
    }
}

#[allow(dead_code)] // 取引先ごとの様式（crate::custom）が使う。カスタマイズが無いビルドでは使われない
pub(crate) fn collapse_ws(s: &str) -> String {
    static WS_RE: LazyLock<Regex> = LazyLock::new(|| compile_static(r"\s+"));
    truncate_chars(&WS_RE.replace_all(s, " "), 255)
}

#[allow(dead_code)] // 取引先ごとの様式（crate::custom）が使う。カスタマイズが無いビルドでは使われない
pub(crate) fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_regexes_compile() {
        // 起動時にすべての静的正規表現がコンパイル可能か検証
        // parse() 関数を呼び出すことで LazyLock が評価される
        let _ = parse("");
    }

    #[test]
    fn parse_generic_falls_back_when_no_known_format() {
        let text = "注文番号: XYZ-123\n2026/4/1\n単価：￥500,000\n140h〜200h";
        let result = parse(text);
        assert_eq!(result.format, "generic");
        assert_eq!(result.client_order_number.as_deref(), Some("XYZ-123"));
        assert_eq!(result.unit_price, Some(500_000));
    }

}
