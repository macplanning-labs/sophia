/// infrastructure/attachment_parsers/payment_notice_pdf_parser.rs — 支払通知書・請求書PDF自動パーサー
///
/// EDI_MP `billing/services/payment_notice_pdf_parser.py`（イービジネスEDI-OASIS形式）を
/// Rustへ移植したもの。テキスト抽出は呼び出し元(mod.rs)がpdf-extractで行う。

use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;
use serde::Serialize;
use std::sync::LazyLock;

#[derive(Debug, Clone, Serialize)]
pub struct PaymentNoticeItem {
    pub product_name: String,
    pub unit_price: i64,
    pub man_month: Decimal,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParsedPaymentNotice {
    pub invoice_number: Option<String>,
    pub issue_date: Option<NaiveDate>,
    pub due_date: Option<NaiveDate>,
    pub target_month: Option<NaiveDate>,
    pub person_name: Option<String>,
    pub project_name: Option<String>,
    pub items: Vec<PaymentNoticeItem>,
    pub subtotal: Option<i64>,
    pub tax_amount: Option<i64>,
    pub total: Option<i64>,
}

fn parse_amount(s: &str) -> Option<i64> {
    s.replace(',', "").parse::<i64>().ok()
}

fn ymd(y: &str, m: &str, d: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// 抽出済みPDFテキストをパースする
pub fn parse(text: &str) -> ParsedPaymentNotice {
    let mut result = ParsedPaymentNotice::default();

    static INVOICE_NO_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:請求番号|注[⽂文]番号|伝票番号)[：:\s]*([A-Z0-9\-]+)").unwrap()
    });
    if let Some(c) = INVOICE_NO_RE.captures(text) {
        result.invoice_number = Some(c[1].trim().to_string());
    }

    static ISSUE_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:請求日|発行日|作成日)[：:\s]*(\d{4})[年/](\d{1,2})[月/](\d{1,2})").unwrap()
    });
    if let Some(c) = ISSUE_DATE_RE.captures(text) {
        result.issue_date = ymd(&c[1], &c[2], &c[3]);
    }

    static DUE_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:支払期日|支払日|お支払日)[：:\s]*(\d{4})[年/](\d{1,2})[月/](\d{1,2})").unwrap()
    });
    if let Some(c) = DUE_DATE_RE.captures(text) {
        result.due_date = ymd(&c[1], &c[2], &c[3]);
    }

    // 対象月: 作業期間「2026年4月1日 〜 2026年4月30日」の開始日の年月
    static WORK_PERIOD_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(\d{4})[年/](\d{1,2})[月/](\d{1,2})日?\s*[〜～~]\s*(\d{4})[年/](\d{1,2})[月/](\d{1,2})",
        )
        .unwrap()
    });
    if let Some(c) = WORK_PERIOD_RE.captures(text) {
        result.target_month = ymd(&c[1], &c[2], "1");
    }
    // フォールバック: 「2026年04月分」「2026年4月」等
    if result.target_month.is_none() {
        static TARGET_MONTH_FALLBACK_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"(\d{4})[年/](\d{1,2})[月分]").unwrap());
        if let Some(c) = TARGET_MONTH_FALLBACK_RE.captures(text) {
            result.target_month = ymd(&c[1], &c[2], "1");
        }
    }

    static PROJECT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)(?:業務名称|件名|案件名)[：:\s]*\n?\s*(.+?)(?:\n|作業期間)").unwrap()
    });
    static DATE_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}年").unwrap());
    if let Some(c) = PROJECT_NAME_RE.captures(text) {
        let name = c[1].trim();
        if !name.is_empty() && !DATE_START_RE.is_match(name) {
            static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
            result.project_name = Some(truncate_chars(&WS_RE.replace_all(name, " "), 255));
        }
    }

    static PERSON_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:作業責任者|担当者|作業者)[：:\s]*\n?\s*(.+?)(?:\n)").unwrap());
    static PERSON_EXCLUDE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(連絡|委託|業務|￥|\d{4}年)").unwrap());
    if let Some(c) = PERSON_RE.captures(text) {
        let name = c[1].trim();
        if !name.is_empty() && !PERSON_EXCLUDE_RE.is_match(name) {
            result.person_name = Some(name.to_string());
        }
    }

    // ── 金額 ──
    static UNIT_PRICE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:[⽉月]額基本料[⾦金]|単価)[：:\s]*[￥¥]?([\d,]+)").unwrap()
    });
    if let Some(c) = UNIT_PRICE_RE.captures(text) {
        if let Some(unit_price) = parse_amount(&c[1]) {
            result.items.push(PaymentNoticeItem {
                product_name: result
                    .project_name
                    .clone()
                    .unwrap_or_else(|| "SES業務委託".to_string()),
                unit_price,
                man_month: Decimal::new(100, 2), // 1.00
            });
        }
    }

    // 文字クラス[合計金額]*はPython版と同様、これらの文字の任意個の並びを許容する
    // （「税抜合計金額」「税抜金額」等の表記ゆれをまとめて拾うための緩いマッチ）
    static SUBTOTAL_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:税抜[合計金額]*|小計)[：:\s]*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = SUBTOTAL_RE.captures(text) {
        result.subtotal = parse_amount(&c[1]);
    }

    static TAX_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:消費税|税額)[：:\s]*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = TAX_RE.captures(text) {
        result.tax_amount = parse_amount(&c[1]);
    }

    static TOTAL_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:税込[合計金額]*|合計[金額]*|請求金額|ご請求額)[：:\s]*[￥¥]?([\d,]+)").unwrap()
    });
    if let Some(c) = TOTAL_RE.captures(text) {
        result.total = parse_amount(&c[1]);
    }

    // 合計が取れたが小計が取れない場合、税率10%で逆算。逆もまた然り。
    match (result.total, result.subtotal) {
        (Some(total), None) => {
            let subtotal = (total as f64 / 1.1) as i64;
            result.subtotal = Some(subtotal);
            result.tax_amount = Some(total - subtotal);
        }
        (None, Some(subtotal)) => {
            let tax = result.tax_amount.unwrap_or_else(|| (subtotal as f64 * 0.1) as i64);
            result.tax_amount = Some(tax);
            result.total = Some(subtotal + tax);
        }
        _ => {}
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_core_fields_and_reconciles_totals() {
        let text = "請求番号：INV-2026-0001\n\
             請求日：2026年4月30日\n\
             支払期日：2026年5月31日\n\
             業務名称：\nSESシステム開発\n作業期間\n\
             2026年4月1日 〜 2026年4月30日\n\
             作業責任者：\n山田太郎\n\
             月額基本料金：￥800,000\n\
             合計金額：￥880,000";
        let result = parse(text);
        assert_eq!(result.invoice_number.as_deref(), Some("INV-2026-0001"));
        assert_eq!(result.issue_date, NaiveDate::from_ymd_opt(2026, 4, 30));
        assert_eq!(result.due_date, NaiveDate::from_ymd_opt(2026, 5, 31));
        assert_eq!(result.target_month, NaiveDate::from_ymd_opt(2026, 4, 1));
        assert_eq!(result.person_name.as_deref(), Some("山田太郎"));
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].unit_price, 800_000);
        assert_eq!(result.total, Some(880_000));
        // 税率10%からの逆算はf64演算のため厳密に1.1で割り切れず1円ずれることがある
        // （EDI_MP Python版 payment_notice_pdf_parser.py と同じ挙動を意図的に踏襲）
        assert_eq!(result.subtotal, Some(799_999));
        assert_eq!(result.tax_amount, Some(80_001));
    }

    #[test]
    fn falls_back_to_month_only_pattern_for_target_month() {
        let text = "請求番号：INV-0002\n2026年04月分のご請求\n合計：￥100,000";
        let result = parse(text);
        assert_eq!(result.target_month, NaiveDate::from_ymd_opt(2026, 4, 1));
    }

    #[test]
    fn reconciles_subtotal_when_only_total_present() {
        let text = "請求番号：INV-0003\n合計金額：￥110,000";
        let result = parse(text);
        assert_eq!(result.total, Some(110_000));
        assert_eq!(result.subtotal, Some(99_999));
        assert_eq!(result.tax_amount, Some(10_001));
    }
}
