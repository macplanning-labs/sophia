/// infrastructure/attachment_parsers/order_pdf_parser.rs — 注文書PDF自動パーサー
///
/// EDI_MP `billing/services/order_pdf_parser.py`（pdfminer + 正規表現、本番実績あり）を
/// そのままRustへ移植したもの。テキスト抽出自体は呼び出し元(mod.rs)がpdf-extractで行い、
/// この関数は抽出済みテキストを受け取って構造化するだけ。
///
/// 対応形式:
///   - イービジネス形式（注文番号: EB260228I00010 等）
///   - NTP形式（発注書番号: PO-0000000001）
///   - クロスシステムサービス形式（No.2668 / 件名：PM支援業務 等）
///   - 汎用形式（正規表現でベストエフォートパース）
///
/// 文字クラス中の `⽂/⽉/⾦/⾜/⽀` はCJK部首互換文字（U+2E80台）。
/// PDFのフォントサブセット化の都合でpdfminer/pdf-extractがこれらを通常の
/// 「文/月/金/足/支」の代わりに抽出することがあるため、Python版と同様に両方を許容する。

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

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

    if is_ebusiness_format(text) {
        result.format = "ebusiness";
        parse_ebusiness(text, &mut result);
    } else if is_ntp_format(text) {
        result.format = "ntp";
        parse_ntp(text, &mut result);
    } else if is_cross_format(text) {
        result.format = "cross";
        parse_cross(text, &mut result);
    } else {
        result.format = "generic";
        parse_generic(text, &mut result);
    }

    result
}

fn is_ebusiness_format(text: &str) -> bool {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"EB\d{6}[A-Z]\d+").unwrap());
    text.contains("イー・ビジネス") || RE.is_match(text)
}

fn is_ntp_format(text: &str) -> bool {
    text.contains("NTP") || text.contains("PO-")
}

fn is_cross_format(text: &str) -> bool {
    text.contains("クロスシステムサービス")
        || text.contains("CROSS SYSTEM SERVICE")
        || text.contains("CROSS SYSTEM")
}

fn parse_amount(s: &str) -> Option<i64> {
    s.replace(',', "").parse::<i64>().ok()
}

fn ymd(y: &str, m: &str, d: &str) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)
}

fn parse_ebusiness(text: &str, result: &mut ParsedOrder) {
    static ORDER_NO_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:注[⽂文]番号[：:]?\s*)([A-Z]{2}\d{6}[A-Z]\d+)").unwrap());
    if let Some(c) = ORDER_NO_RE.captures(text) {
        result.client_order_number = c.get(1).map(|m| m.as_str().to_string());
    }

    static ORDER_DATE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d{4})年(\d{1,2})[⽉月](\d{1,2})日").unwrap());
    if let Some(c) = ORDER_DATE_RE.captures(text) {
        result.order_date = ymd(&c[1], &c[2], &c[3]);
    }

    // 業務名称: pdfminerは「業務名称\n作業期間\n<業務名>\n<日付>」の順で抽出することが多い
    static PROJECT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"業務名称\s*\n+\s*作業期間\s*\n+\s*(.+?)\s*\n").unwrap()
    });
    static DATE_START_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}年").unwrap());
    if let Some(c) = PROJECT_NAME_RE.captures(text) {
        let name = c[1].trim();
        if !DATE_START_RE.is_match(name) {
            result.project_name = Some(collapse_ws(name));
        }
    }
    // フォールバック: 従来のパターン
    if result.project_name.is_none() {
        static FALLBACK_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"(?s)(?:業務名称|件名)\s*\n?\s*(.+?)(?:\n|作業期間)").unwrap()
        });
        if let Some(c) = FALLBACK_RE.captures(text) {
            let name = c[1].trim();
            if !name.is_empty() && name != "作業期間" {
                result.project_name = Some(collapse_ws(name));
            }
        }
    }

    static WORK_PERIOD_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(\d{4})年(\d{1,2})[⽉月](\d{1,2})日\s*[〜～~]\s*(\d{4})年(\d{1,2})[⽉月](\d{1,2})日",
        )
        .unwrap()
    });
    if let Some(c) = WORK_PERIOD_RE.captures(text) {
        result.work_start = ymd(&c[1], &c[2], &c[3]);
        result.work_end = ymd(&c[4], &c[5], &c[6]);
    }

    static UNIT_PRICE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"[⽉月]額基本料[⾦金][：:]?\s*[￥¥]?([\d,]+)").unwrap()
    });
    if let Some(c) = UNIT_PRICE_RE.captures(text) {
        result.unit_price = parse_amount(&c[1]);
    }

    static TIME_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"基準時間[：:]?\s*([\d.]+)\s*h?\s*[〜～~]\s*([\d.]+)\s*h?").unwrap()
    });
    if let Some(c) = TIME_RANGE_RE.captures(text) {
        result.time_lower = c[1].parse().ok();
        result.time_upper = c[2].parse().ok();
    }

    static EXCESS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"超過単価[：:]?\s*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = EXCESS_RE.captures(text) {
        result.excess_rate = parse_amount(&c[1]);
    }

    static SHORTAGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:不[⾜足]|控除)単価[：:]?\s*[￥¥]?([\d,]+)").unwrap()
    });
    if let Some(c) = SHORTAGE_RE.captures(text) {
        result.shortage_rate = parse_amount(&c[1]);
    }

    static PERSON_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"作業責任者\s*\n+\s*(.+?)(?:\s*\n)").unwrap());
    static PERSON_EXCLUDE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(連絡|委託|業務|￥|\d{4}年)").unwrap());
    if let Some(c) = PERSON_RE.captures(text) {
        let name = c[1].trim();
        if !name.is_empty() && !PERSON_EXCLUDE_RE.is_match(name) {
            result.person_name = Some(name.to_string());
        }
    }

    static PAYMENT_TERMS_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?s)(?:⽀払|支払)条件\s*\n?\s*(.+?)(?:\n|①)").unwrap()
    });
    if let Some(c) = PAYMENT_TERMS_RE.captures(text) {
        result.payment_terms = Some(truncate_chars(c[1].trim(), 255));
    }
}

fn parse_ntp(text: &str, result: &mut ParsedOrder) {
    static ORDER_NO_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(PO-\d+)").unwrap());
    if let Some(c) = ORDER_NO_RE.captures(text) {
        result.client_order_number = Some(c[1].to_string());
    }

    static ISO_DATE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d{4}-\d{2}-\d{2})").unwrap());
    if let Some(c) = ISO_DATE_RE.captures(text) {
        result.order_date = NaiveDate::parse_from_str(&c[1], "%Y-%m-%d").ok();
    }

    // 件名（案件名） — 「件名」を含む行の次の非空行
    for (i, line) in text.lines().enumerate() {
        if line.contains("件名") {
            for candidate in text.lines().skip(i + 1).take(2) {
                if !candidate.trim().is_empty() {
                    result.project_name = Some(truncate_chars(candidate.trim(), 255));
                    break;
                }
            }
            break;
        }
    }
    if result.project_name.is_none() {
        static PJ_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"([^\n]*(?:PJ|プロジェクト|案件)[^\n]*)").unwrap());
        if let Some(c) = PJ_RE.captures(text) {
            result.project_name = Some(truncate_chars(c[1].trim(), 255));
        }
    }

    static UNIT_PRICE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"基本[⽉月]?額?単価[：:]?\s*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = UNIT_PRICE_RE.captures(text) {
        result.unit_price = parse_amount(&c[1]);
    } else {
        static AMOUNT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"([\d,]{5,})円").unwrap());
        if let Some(c) = AMOUNT_RE.captures(text) {
            result.unit_price = parse_amount(&c[1]);
        }
    }

    static TIME_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"精算条件[：:]?\s*([\d.]+)\s*h?\s*[〜～~ー]\s*([\d.]+)\s*h?").unwrap()
    });
    if let Some(c) = TIME_RANGE_RE.captures(text) {
        result.time_lower = c[1].parse().ok();
        result.time_upper = c[2].parse().ok();
    }

    static EXCESS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"超過控除単価[：:]?\s*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = EXCESS_RE.captures(text) {
        let rate = parse_amount(&c[1]);
        result.excess_rate = rate;
        result.shortage_rate = rate; // NTPは超過控除同額
    }

    // 納品期限: 全ISO日付のうち最後のものを作業終了日、その年月初日を作業開始日とする
    static ALL_ISO_DATES_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d{4}-\d{2}-\d{2})").unwrap());
    let dates: Vec<&str> = ALL_ISO_DATES_RE
        .captures_iter(text)
        .filter_map(|c| c.get(1).map(|m| m.as_str()))
        .collect();
    if dates.len() >= 2 {
        if let Some(last) = dates.last() {
            result.work_end = NaiveDate::parse_from_str(last, "%Y-%m-%d").ok();
            if last.len() >= 7 {
                if let Ok(month_start) = NaiveDate::parse_from_str(&format!("{}-01", &last[..7]), "%Y-%m-%d") {
                    result.work_start = Some(month_start);
                }
            }
        }
    }
}

fn parse_cross(text: &str, result: &mut ParsedOrder) {
    // No.2668 / No．2668
    static ORDER_NO_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"No[.．]\s*([0-9A-Za-z\-]+)").unwrap());
    if let Some(c) = ORDER_NO_RE.captures(text) {
        result.client_order_number = Some(c[1].to_string());
    }

    static ORDER_DATE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"発注日[：:\s]*(\d{4})年(\d{1,2})[⽉月](\d{1,2})日").unwrap()
    });
    if let Some(c) = ORDER_DATE_RE.captures(text) {
        result.order_date = ymd(&c[1], &c[2], &c[3]);
    }

    static PROJECT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"件名[：:\s]*([^\n]+)").unwrap());
    if let Some(c) = PROJECT_RE.captures(text) {
        let name = collapse_ws(c[1].trim());
        if !name.is_empty() {
            result.project_name = Some(name);
        }
    }
    if result.project_name.is_none() {
        static ABSTRACT_RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"摘要[：:\s]*([^\n]+)").unwrap());
        if let Some(c) = ABSTRACT_RE.captures(text) {
            let name = collapse_ws(c[1].trim());
            if !name.is_empty() {
                result.project_name = Some(name);
            }
        }
    }

    static WORK_PERIOD_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"契約期間[：:\s]*(\d{4})年(\d{1,2})[⽉月](\d{1,2})日\s*[〜～~]\s*(\d{4})年(\d{1,2})[⽉月](\d{1,2})日",
        )
        .unwrap()
    });
    if let Some(c) = WORK_PERIOD_RE.captures(text) {
        result.work_start = ymd(&c[1], &c[2], &c[3]);
        result.work_end = ymd(&c[4], &c[5], &c[6]);
    }

    // 明細の単価列（数量×単価）を優先。なければ月額表記
    static UNIT_PRICE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"単価[^\d]{0,20}([\d,]+)").unwrap()
    });
    if let Some(c) = UNIT_PRICE_RE.captures(text) {
        result.unit_price = parse_amount(&c[1]);
    }
    if result.unit_price.is_none() {
        static MONTHLY_RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"[⽉月]額[：:\s]*[￥¥]?([\d,]+)").unwrap()
        });
        if let Some(c) = MONTHLY_RE.captures(text) {
            result.unit_price = parse_amount(&c[1]);
        }
    }

    static TIME_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"精算条件[：:\s]*([\d.]+)\s*時間?\s*[〜～~]\s*([\d.]+)\s*時間?").unwrap()
    });
    if let Some(c) = TIME_RANGE_RE.captures(text) {
        result.time_lower = c[1].parse().ok();
        result.time_upper = c[2].parse().ok();
    }

    static EXCESS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"超過[：:\s]*([\d,]+)\s*円").unwrap());
    if let Some(c) = EXCESS_RE.captures(text) {
        result.excess_rate = parse_amount(&c[1]);
    }

    static SHORTAGE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"控除[：:\s]*([\d,]+)\s*円").unwrap());
    if let Some(c) = SHORTAGE_RE.captures(text) {
        result.shortage_rate = parse_amount(&c[1]);
    }

    static PAYMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"支払条件[：:\s]*([^\n]+)").unwrap()
    });
    if let Some(c) = PAYMENT_RE.captures(text) {
        result.payment_terms = Some(truncate_chars(c[1].trim(), 255));
    }
}

fn parse_generic(text: &str, result: &mut ParsedOrder) {
    static ORDER_NO_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:注文番号|発注番号|PO番号)[：:\s]*([A-Za-z0-9\-]+)").unwrap()
    });
    if let Some(c) = ORDER_NO_RE.captures(text) {
        result.client_order_number = Some(c[1].to_string());
    }

    static DATE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(\d{4})[/\-年](\d{1,2})[/\-月](\d{1,2})").unwrap());
    if let Some(c) = DATE_RE.captures(text) {
        result.order_date = ymd(&c[1], &c[2], &c[3]);
    }

    static UNIT_PRICE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:単価|月額)[：:\s]*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = UNIT_PRICE_RE.captures(text) {
        result.unit_price = parse_amount(&c[1]);
    }

    static TIME_RANGE_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"([\d.]+)\s*h?\s*[〜～~ー]\s*([\d.]+)\s*h?").unwrap()
    });
    if let Some(c) = TIME_RANGE_RE.captures(text) {
        result.time_lower = c[1].parse().ok();
        result.time_upper = c[2].parse().ok();
    }

    static EXCESS_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"超過[^\d]*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = EXCESS_RE.captures(text) {
        result.excess_rate = parse_amount(&c[1]);
    }

    static SHORTAGE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:控除|不足)[^\d]*[￥¥]?([\d,]+)").unwrap());
    if let Some(c) = SHORTAGE_RE.captures(text) {
        result.shortage_rate = parse_amount(&c[1]);
    }
}

fn collapse_ws(s: &str) -> String {
    static WS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
    truncate_chars(&WS_RE.replace_all(s, " "), 255)
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ebusiness_format_by_order_number() {
        assert!(is_ebusiness_format("注文番号: EB260228I00010"));
    }

    #[test]
    fn detects_ntp_format_by_prefix() {
        assert!(is_ntp_format("発注書番号: PO-0000000001"));
    }

    #[test]
    fn parse_ebusiness_extracts_core_fields() {
        let text = "注文番号：EB260228I00010\n\
             2026年2月28日\n\
             業務名称\n作業期間\nSESシステム開発\n2026年3月1日 〜 2026年3月31日\n\
             月額基本料金：￥800,000\n\
             基準時間：140.0h〜200.0h\n\
             超過単価：￥4,000\n\
             不足単価：￥5,710\n\
             作業責任者\n山田太郎\n\
             支払条件\n毎月末日締め翌月末日払い\n①備考";
        let result = parse(text);
        assert_eq!(result.format, "ebusiness");
        assert_eq!(result.client_order_number.as_deref(), Some("EB260228I00010"));
        assert_eq!(result.order_date, NaiveDate::from_ymd_opt(2026, 2, 28));
        assert_eq!(result.work_start, NaiveDate::from_ymd_opt(2026, 3, 1));
        assert_eq!(result.work_end, NaiveDate::from_ymd_opt(2026, 3, 31));
        assert_eq!(result.unit_price, Some(800_000));
        assert_eq!(result.time_lower, Some(140.0));
        assert_eq!(result.time_upper, Some(200.0));
        assert_eq!(result.excess_rate, Some(4_000));
        assert_eq!(result.shortage_rate, Some(5_710));
        assert_eq!(result.person_name.as_deref(), Some("山田太郎"));
    }

    #[test]
    fn parse_ntp_extracts_order_number_and_rates() {
        let text = "発注書番号：PO-0000000001\n2024-11-13\n件名\n\nEB向けPJ\n\
             基本月額単価：￥750,000\n精算条件：140h～200h\n超過控除単価：￥4,410\n\
             2024-11-01\n2024-11-30";
        let result = parse(text);
        assert_eq!(result.format, "ntp");
        assert_eq!(result.client_order_number.as_deref(), Some("PO-0000000001"));
        assert_eq!(result.unit_price, Some(750_000));
        assert_eq!(result.time_lower, Some(140.0));
        assert_eq!(result.time_upper, Some(200.0));
        assert_eq!(result.excess_rate, Some(4_410));
        assert_eq!(result.shortage_rate, Some(4_410));
        assert_eq!(result.work_end, NaiveDate::from_ymd_opt(2024, 11, 30));
        assert_eq!(result.work_start, NaiveDate::from_ymd_opt(2024, 11, 1));
    }

    #[test]
    fn parse_generic_falls_back_when_no_known_format() {
        let text = "注文番号: XYZ-123\n2026/4/1\n単価：￥500,000\n140h〜200h";
        let result = parse(text);
        assert_eq!(result.format, "generic");
        assert_eq!(result.client_order_number.as_deref(), Some("XYZ-123"));
        assert_eq!(result.unit_price, Some(500_000));
    }

    #[test]
    fn parse_cross_extracts_core_fields() {
        let text = "発注書\n\
             クロスシステムサービス株式会社\n\
             CROSS SYSTEM SERVICE\n\
             No.2668\n\
             発注日：2026年6月29日\n\
             件名：PM支援業務\n\
             契約期間：2026年7月1日 ～ 2026年9月30日\n\
             摘要\nPM支援業務\n数量 3 単価 900,000 金額 2,700,000\n\
             精算条件：140時間～180時間\n\
             超過：5,000円／時間\n\
             控除：6,420円／時間\n\
             支払条件：月末締め翌月末払い";
        let result = parse(text);
        assert_eq!(result.format, "cross");
        assert_eq!(result.client_order_number.as_deref(), Some("2668"));
        assert_eq!(result.order_date, NaiveDate::from_ymd_opt(2026, 6, 29));
        assert_eq!(result.project_name.as_deref(), Some("PM支援業務"));
        assert_eq!(result.work_start, NaiveDate::from_ymd_opt(2026, 7, 1));
        assert_eq!(result.work_end, NaiveDate::from_ymd_opt(2026, 9, 30));
        assert_eq!(result.unit_price, Some(900_000));
        assert_eq!(result.time_lower, Some(140.0));
        assert_eq!(result.time_upper, Some(180.0));
        assert_eq!(result.excess_rate, Some(5_000));
        assert_eq!(result.shortage_rate, Some(6_420));
    }
}
