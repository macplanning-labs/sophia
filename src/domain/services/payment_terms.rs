//! domain/services/payment_terms.rs — 支払条件の文字(例: 「毎月末日締め翌月末日払い」)から、支払期日を出す(DEMO-000200)。
//!
//! 請求書・支払通知の支払期日(settlement_dashboard)と、ホームの期限(home/dashboard)が同じ読み方をするよう、ここに1本化する。
//! 以前は「15日」「末日」の文字があるかだけで判断していたため、20日・25日払いを月末払いと、「15日締め」を15日払いと取り違えていた。
//! DB に触れない純粋な関数。

use chrono::{Datelike, NaiveDate};

/// 支払日
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayDay {
    /// その月の N 日(その月に無い日は月末)
    Day(u32),
    /// 月末
    EndOfMonth,
}

/// 支払条件(締めた月の何か月後の、何日に払うか)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentTerms {
    pub months_after: u32,
    pub pay_day: PayDay,
}

/// 条件が空・読めないときの既定(翌月末日払い。m_client_contract / t_purchase_order の列の既定値と同じ)
pub const DEFAULT_TERMS: PaymentTerms = PaymentTerms { months_after: 1, pay_day: PayDay::EndOfMonth };

/// 全角の数字・空白を半角にする
fn normalize(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            '\u{3000}' => ' ',
            _ => c,
        })
        .collect()
}

/// 「…の直前にある数字」を読む(例: "翌月25日払い" の "日払" の前 → 25)
fn number_before(s: &str, idx: usize) -> Option<u32> {
    let head = &s[..idx];
    let digits: String = head.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect();
    if digits.is_empty() { None } else { digits.parse().ok() }
}

/// 支払条件の文字を読む。支払いの部分(「払」)が見つからない・読めないときは None。
///
/// 読み方: 「締め」より後ろ(無ければ全体)を支払いの部分とし、
/// - 何か月後: 「翌々月」=2、「翌月」=1、「当月」=0、「N か月後 / Nヶ月後 / Nカ月後」=N。どれも無ければ 1
/// - 支払日: 「末」(末日・月末)=月末、「N日」=N(1〜31)
pub fn parse_payment_terms(text: &str) -> Option<PaymentTerms> {
    let t = normalize(text);
    let t = t.trim();
    if t.is_empty() {
        return None;
    }
    // 締めの部分を除く(「毎月15日締め」の 15 を支払日と読まないため)
    let pay_part = match t.find("締") {
        Some(i) => &t[i..],
        None => t,
    };
    let pay_idx = pay_part.find('払')?;
    let pay_part = &pay_part[..pay_idx];

    let months_after = if pay_part.contains("翌々月") {
        2
    } else if pay_part.contains("翌月") {
        1
    } else if pay_part.contains("当月") {
        0
    } else if let Some(n) = ["か月後", "ヶ月後", "カ月後", "ケ月後"]
        .iter()
        .find_map(|k| pay_part.find(k).and_then(|i| number_before(pay_part, i)))
    {
        n
    } else {
        1
    };

    let pay_day = if pay_part.ends_with("末") || pay_part.ends_with("末日") || pay_part.contains("月末") {
        PayDay::EndOfMonth
    } else if let Some(i) = pay_part.rfind('日') {
        match number_before(pay_part, i) {
            Some(d) if (1..=31).contains(&d) => PayDay::Day(d),
            _ => return None,
        }
    } else {
        return None;
    };
    Some(PaymentTerms { months_after, pay_day })
}

/// 支払条件の文字から読み、読めなければ既定(翌月末日)を使う。読めなかったことはログに残す
pub fn parse_or_default(text: &str) -> PaymentTerms {
    if text.trim().is_empty() {
        return DEFAULT_TERMS;
    }
    match parse_payment_terms(text) {
        Some(t) => t,
        None => {
            tracing::warn!("[支払条件] 読み取れないため既定(翌月末日払い)で計算します: {:?}", text);
            DEFAULT_TERMS
        }
    }
}

fn last_day(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

/// 締めた日(作業の最終日を含む月)から、支払期日を出す
pub fn due_date(work_end: NaiveDate, terms: &PaymentTerms) -> NaiveDate {
    let total = work_end.month0() + terms.months_after;
    let y = work_end.year() + (total / 12) as i32;
    let m = total % 12 + 1;
    let last = last_day(y, m);
    let d = match terms.pay_day {
        PayDay::EndOfMonth => last,
        PayDay::Day(d) => d.min(last),
    };
    NaiveDate::from_ymd_opt(y, m, d).unwrap_or(work_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }
    fn due(end: NaiveDate, text: &str) -> NaiveDate {
        due_date(end, &parse_or_default(text))
    }

    // ── 本番・ステージングにある書き方(2026-10-10 時点)は、以前と同じ結果 ──
    #[test]
    fn existing_terms_in_production_keep_their_results() {
        let end = ymd(2026, 7, 31);
        assert_eq!(due(end, "毎月末日締め翌月末日払い"), ymd(2026, 8, 31));
        assert_eq!(due(end, "毎月末日締め翌月末日払い（税別）"), ymd(2026, 8, 31));
        assert_eq!(due(end, "毎月月末締め翌々月15日払い"), ymd(2026, 9, 15));
        assert_eq!(due(end, "毎月末日締め翌々月１５日払い（税別）"), ymd(2026, 9, 15));
        assert_eq!(due(end, "毎月末日締め翌々月15日払い（税別）"), ymd(2026, 9, 15));
        assert_eq!(due(end, ""), ymd(2026, 8, 31), "空は翌月末日");
        assert_eq!(due(end, "翌々月15日払い"), ymd(2026, 9, 15));
        assert_eq!(due(end, "翌月末日払い"), ymd(2026, 8, 31));
    }

    // ── 以前は取り違えていた書き方 ──
    #[test]
    fn pay_days_other_than_15th_and_end_of_month() {
        let end = ymd(2026, 7, 31);
        assert_eq!(due(end, "毎月末日締め翌月25日払い"), ymd(2026, 8, 25));
        assert_eq!(due(end, "毎月末日締め翌月20日払い"), ymd(2026, 8, 20));
        assert_eq!(due(end, "末締め翌月10日払い"), ymd(2026, 8, 10));
    }

    #[test]
    fn closing_day_15th_is_not_read_as_pay_day() {
        assert_eq!(due(ymd(2026, 7, 15), "毎月15日締め翌月末日払い"), ymd(2026, 8, 31));
        assert_eq!(due(ymd(2026, 7, 20), "20日締め翌月10日払い"), ymd(2026, 8, 10));
    }

    #[test]
    fn day_missing_in_the_month_falls_back_to_end_of_month() {
        assert_eq!(due(ymd(2026, 8, 31), "毎月末日締め翌月31日払い"), ymd(2026, 9, 30));
        assert_eq!(due(ymd(2027, 1, 31), "翌月末日払い"), ymd(2027, 2, 28));
        assert_eq!(due(ymd(2028, 1, 31), "翌月末日払い"), ymd(2028, 2, 29), "うるう年");
    }

    #[test]
    fn year_boundary_and_months_after() {
        assert_eq!(due(ymd(2026, 11, 30), "翌々月15日払い"), ymd(2027, 1, 15));
        assert_eq!(due(ymd(2026, 12, 31), "翌月末日払い"), ymd(2027, 1, 31));
        assert_eq!(due(ymd(2026, 7, 31), "月末締め当月末日払い"), ymd(2026, 7, 31));
        assert_eq!(due(ymd(2026, 7, 31), "月末締め3ヶ月後10日払い"), ymd(2026, 10, 10));
    }

    #[test]
    fn unreadable_terms_use_the_default() {
        assert_eq!(parse_payment_terms("別途協議"), None);
        assert_eq!(parse_payment_terms("翌月払い"), None, "支払日が無い");
        assert_eq!(due(ymd(2026, 7, 31), "別途協議"), ymd(2026, 8, 31), "読めなければ翌月末日");
    }

    #[test]
    fn parse_results() {
        assert_eq!(
            parse_payment_terms("毎月末日締め翌々月15日払い"),
            Some(PaymentTerms { months_after: 2, pay_day: PayDay::Day(15) })
        );
        assert_eq!(parse_payment_terms("翌月末払い"), Some(PaymentTerms { months_after: 1, pay_day: PayDay::EndOfMonth }));
    }
}
