/// domain/services/jst.rs — 日本時間(+09:00)で時刻を文字にする(DEMO-000220)
///
/// DB の時刻は世界標準時(UTC)で、サーバのコンテナの時計も UTC(`Local::now()` も UTC になる)。
/// そのまま `format` すると、画面・メール・通知に出す時刻が9時間ずれる。人に見せる時刻は、必ずここを通す。
use chrono::{DateTime, FixedOffset, Offset, TimeZone, Utc};

/// 日本時間のずれ(+09:00)。作れないときは UTC(落とさない)
pub fn offset() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).unwrap_or_else(|| Utc.fix())
}

/// 日本時間の「いま」
pub fn now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&offset())
}

/// 日本時間に直して、指定の書式で文字にする(例: `"%Y年%m月%d日 %H:%M"`)
pub fn format<T: TimeZone>(t: DateTime<T>, fmt: &str) -> String {
    t.with_timezone(&offset()).format(fmt).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn utc_0510_is_jst_1410() {
        // 2026-10-11 の staging で、承諾の時刻が 14:10 と出た(本当は 23:10。DEMO-000220)。UTC → 日本時間の変換を固定する
        assert_eq!(format(utc("2026-10-11T14:10:00Z"), "%Y年%m月%d日 %H:%M"), "2026年10月11日 23:10");
        assert_eq!(format(utc("2026-10-12T05:10:00Z"), "%H:%M"), "14:10");
    }

    #[test]
    fn crosses_midnight_into_the_next_day() {
        // UTC の 15:00 以降は、日本ではもう翌日
        assert_eq!(format(utc("2026-10-12T15:00:00Z"), "%Y-%m-%d %H:%M"), "2026-10-13 00:00");
    }

    #[test]
    fn works_for_any_timezone_input() {
        let local = chrono::FixedOffset::west_opt(5 * 3600).unwrap().with_ymd_and_hms(2026, 10, 12, 0, 0, 0).unwrap();
        assert_eq!(format(local, "%Y-%m-%d %H:%M"), "2026-10-12 14:00");
    }

    #[test]
    fn now_is_plus_nine() {
        assert_eq!(now().offset().local_minus_utc(), 9 * 3600);
    }
}
