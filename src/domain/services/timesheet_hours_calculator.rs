/// domain/services/timesheet_hours_calculator.rs
///
/// 社員自己申告（`/my-timesheet`）・給与計算向けに、日次データから
/// 労基法・会社勤務条件に基づく時間外 / 深夜 / 休日の実労働時間を算出する。
///
/// 案件紐付けの稼働報告アップロードでは使わない（精算超過は受注契約の上下限で算出）。
///
/// 会社条件:
/// - 所定: 9:00–18:00（休憩 12:00–13:00）、1日 8 時間
/// - 休日: 土・祝 = 法定外休日、日 = 法定休日
/// - 深夜: 22:00–翌 5:00
///
/// 時間の定義（給与カラム用・重複あり）:
/// - overtime_hours … 平日（労働日）の実労働のうち 8h 超過分
/// - night_hours    … 22:00–5:00 に重なる実労働（曜日不問）
/// - holiday_hours  … 土・日・祝の実労働（法定外・法定とも）

use chrono::{Datelike, NaiveDate, Weekday};
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

use crate::domain::services::excel_parser::DailyEntry;

/// 1 日の所定労働時間（時間）
pub const STANDARD_DAILY_HOURS: f64 = 8.0;
/// 所定休憩（分、正午からのオフセット）
const DEFAULT_BREAK_START_MIN: i32 = 12 * 60;
const DEFAULT_BREAK_END_MIN: i32 = 13 * 60;
/// 深夜帯
const NIGHT_START_MIN: i32 = 22 * 60; // 22:00
const NIGHT_END_MIN: i32 = 5 * 60; // 05:00（翌日）

/// 日次から集計した時間外・深夜・休日時間
#[derive(Debug, Clone, PartialEq)]
pub struct TimesheetHourBreakdown {
    pub overtime_hours: Decimal,
    pub night_hours: Decimal,
    pub holiday_hours: Decimal,
    /// 休憩控除後の実労働合計
    pub total_worked_hours: Decimal,
}

impl Default for TimesheetHourBreakdown {
    fn default() -> Self {
        Self {
            overtime_hours: Decimal::ZERO,
            night_hours: Decimal::ZERO,
            holiday_hours: Decimal::ZERO,
            total_worked_hours: Decimal::ZERO,
        }
    }
}

/// 勤務日の区分
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkDayKind {
    /// 平日（月〜金かつ祝日でない）
    Weekday,
    /// 土曜日または祝日（日曜日以外）= 法定外休日
    NonStatutoryHoliday,
    /// 日曜日 = 法定休日（祝日と重なってもこちら）
    StatutoryHoliday,
}

/// 日付から勤務区分を判定する。
pub fn classify_work_day(date: NaiveDate) -> WorkDayKind {
    if date.weekday() == Weekday::Sun {
        return WorkDayKind::StatutoryHoliday;
    }
    let is_holiday = jpholiday::is_holiday(
        jpholiday::Date::new(date.year(), date.month(), date.day())
            .expect("chrono::NaiveDate is always a valid calendar date"),
    );
    if date.weekday() == Weekday::Sat || is_holiday {
        WorkDayKind::NonStatutoryHoliday
    } else {
        WorkDayKind::Weekday
    }
}

/// 日次エントリ列から時間外・深夜・休日時間を集計する。
pub fn calculate_hours_from_daily(entries: &[DailyEntry]) -> TimesheetHourBreakdown {
    let mut overtime_min: i64 = 0;
    let mut night_min: i64 = 0;
    let mut holiday_min: i64 = 0;
    let mut total_min: i64 = 0;

    for entry in entries {
        let Some(day) = analyze_daily_entry(entry) else {
            continue;
        };
        total_min += day.worked_minutes;
        night_min += day.night_minutes;
        match day.kind {
            WorkDayKind::Weekday => {
                let ot = (day.worked_minutes - (STANDARD_DAILY_HOURS * 60.0) as i64).max(0);
                overtime_min += ot;
            }
            WorkDayKind::NonStatutoryHoliday | WorkDayKind::StatutoryHoliday => {
                holiday_min += day.worked_minutes;
            }
        }
    }

    TimesheetHourBreakdown {
        overtime_hours: minutes_to_decimal(overtime_min),
        night_hours: minutes_to_decimal(night_min),
        holiday_hours: minutes_to_decimal(holiday_min),
        total_worked_hours: minutes_to_decimal(total_min),
    }
}

struct DayAnalysis {
    kind: WorkDayKind,
    worked_minutes: i64,
    night_minutes: i64,
}

fn analyze_daily_entry(entry: &DailyEntry) -> Option<DayAnalysis> {
    let date = parse_date(&entry.date)?;
    let kind = classify_work_day(date);

    // 開始・終了が両方あれば区間計算。なければ hours のみで概算（深夜は 0）
    let (worked, night) = match (parse_hhmm(&entry.start), parse_hhmm(&entry.end)) {
        (Some(start), Some(end)) => {
            let end = if end <= start { end + 24 * 60 } else { end };
            let worked_intervals = subtract_default_break(start, end);
            let worked: i64 = worked_intervals.iter().map(|(a, b)| (b - a) as i64).sum();
            let night = night_overlap_minutes(&worked_intervals);
            (worked, night)
        }
        _ => {
            if entry.hours <= 0.0 {
                return None;
            }
            let worked = (entry.hours * 60.0).round() as i64;
            (worked, 0)
        }
    };

    if worked <= 0 {
        return None;
    }

    Some(DayAnalysis {
        kind,
        worked_minutes: worked,
        night_minutes: night,
    })
}

/// 所定休憩 12:00–13:00 を勤務区間から差し引く。
/// 複数区間になる場合あり（休憩をまたぐとき）。
fn subtract_default_break(start: i32, end: i32) -> Vec<(i32, i32)> {
    subtract_interval(start, end, DEFAULT_BREAK_START_MIN, DEFAULT_BREAK_END_MIN)
}

fn subtract_interval(start: i32, end: i32, br_start: i32, br_end: i32) -> Vec<(i32, i32)> {
    if end <= start {
        return vec![];
    }
    // 休憩は当日 12–13。日をまたぐ勤務では「当日」「翌日」の両方を差し引く
    let mut breaks = vec![(br_start, br_end)];
    if end > 24 * 60 {
        breaks.push((br_start + 24 * 60, br_end + 24 * 60));
    }

    let mut intervals = vec![(start, end)];
    for (bs, be) in breaks {
        let mut next = Vec::new();
        for (a, b) in intervals {
            next.extend(subtract_one(a, b, bs, be));
        }
        intervals = next;
    }
    intervals
}

fn subtract_one(a: i32, b: i32, bs: i32, be: i32) -> Vec<(i32, i32)> {
    let overlap_start = a.max(bs);
    let overlap_end = b.min(be);
    if overlap_start >= overlap_end {
        return vec![(a, b)];
    }
    let mut out = Vec::new();
    if a < overlap_start {
        out.push((a, overlap_start));
    }
    if overlap_end < b {
        out.push((overlap_end, b));
    }
    out
}

/// 勤務区間と深夜帯（22:00–24:00 / 0:00–5:00、翌日分含む）の重なり分（分）
fn night_overlap_minutes(intervals: &[(i32, i32)]) -> i64 {
    let mut total = 0i64;
    for &(a, b) in intervals {
        // 当日深夜前半 22:00–24:00、翌日 0:00–5:00、および +24h シフト分
        for day_offset in [0, 24 * 60] {
            total += overlap_len(a, b, NIGHT_START_MIN + day_offset, 24 * 60 + day_offset) as i64;
            total += overlap_len(a, b, day_offset, NIGHT_END_MIN + day_offset) as i64;
        }
    }
    total
}

fn overlap_len(a: i32, b: i32, c: i32, d: i32) -> i32 {
    (b.min(d) - a.max(c)).max(0)
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d"))
        .ok()
}

/// "9:00" / "09:00" / "18:00" などを分に変換
fn parse_hhmm(s: &str) -> Option<i32> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return None;
    }
    let h: i32 = parts[0].trim().parse().ok()?;
    let m: i32 = parts[1].trim().parse().ok()?;
    if !(0..=47).contains(&h) || !(0..60).contains(&m) {
        return None;
    }
    Some(h * 60 + m)
}

fn minutes_to_decimal(minutes: i64) -> Decimal {
    let hours = minutes as f64 / 60.0;
    Decimal::from_f64(hours)
        .unwrap_or(Decimal::ZERO)
        .round_dp(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(date: &str, day_name: &str, hours: f64, start: &str, end: &str) -> DailyEntry {
        DailyEntry {
            date: date.to_string(),
            day_name: day_name.to_string(),
            hours,
            start: start.to_string(),
            end: end.to_string(),
        }
    }

    #[test]
    fn weekday_standard_no_ot_no_night() {
        // 2026-07-01 は水曜
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-01",
            "水",
            8.0,
            "09:00",
            "18:00",
        )]);
        assert_eq!(r.overtime_hours, Decimal::ZERO);
        assert_eq!(r.night_hours, Decimal::ZERO);
        assert_eq!(r.holiday_hours, Decimal::ZERO);
        assert_eq!(r.total_worked_hours, Decimal::new(8, 0));
    }

    #[test]
    fn weekday_overtime_until_20() {
        // 9–20（休憩1h）= 10h → OT 2h
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-01",
            "水",
            10.0,
            "09:00",
            "20:00",
        )]);
        assert_eq!(r.overtime_hours, Decimal::new(2, 0));
        assert_eq!(r.night_hours, Decimal::ZERO);
        assert_eq!(r.holiday_hours, Decimal::ZERO);
    }

    #[test]
    fn weekday_night_overtime() {
        // 9:00–23:00（休憩1h）= 13h → OT 5h、深夜 22–23 = 1h
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-01",
            "水",
            13.0,
            "09:00",
            "23:00",
        )]);
        assert_eq!(r.overtime_hours, Decimal::new(5, 0));
        assert_eq!(r.night_hours, Decimal::new(1, 0));
    }

    #[test]
    fn overnight_into_night_band() {
        // 21:00–翌2:00（休憩なし想定で 12–13 は勤務外）= 5h、深夜 22–2 = 4h
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-01",
            "水",
            5.0,
            "21:00",
            "02:00",
        )]);
        assert_eq!(r.total_worked_hours, Decimal::new(5, 0));
        assert_eq!(r.night_hours, Decimal::new(4, 0));
        // 5h < 8h → OT なし
        assert_eq!(r.overtime_hours, Decimal::ZERO);
    }

    #[test]
    fn saturday_all_counts_as_holiday() {
        // 2026-07-04 土曜
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-04",
            "土",
            8.0,
            "09:00",
            "18:00",
        )]);
        assert_eq!(r.holiday_hours, Decimal::new(8, 0));
        assert_eq!(r.overtime_hours, Decimal::ZERO);
    }

    #[test]
    fn sunday_is_statutory_holiday_hours() {
        // 2026-07-05 日曜
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-05",
            "日",
            8.0,
            "09:00",
            "18:00",
        )]);
        assert_eq!(r.holiday_hours, Decimal::new(8, 0));
        assert_eq!(r.overtime_hours, Decimal::ZERO);
    }

    #[test]
    fn national_holiday_weekday_counts_as_holiday() {
        // 2026-07-20 海の日（月）
        let r = calculate_hours_from_daily(&[entry(
            "2026-07-20",
            "月",
            8.0,
            "09:00",
            "18:00",
        )]);
        assert_eq!(classify_work_day(NaiveDate::from_ymd_opt(2026, 7, 20).unwrap()), WorkDayKind::NonStatutoryHoliday);
        assert_eq!(r.holiday_hours, Decimal::new(8, 0));
        assert_eq!(r.overtime_hours, Decimal::ZERO);
    }

    #[test]
    fn fallback_hours_without_times_no_night() {
        let r = calculate_hours_from_daily(&[entry("2026-07-01", "水", 10.0, "", "")]);
        assert_eq!(r.overtime_hours, Decimal::new(2, 0));
        assert_eq!(r.night_hours, Decimal::ZERO);
    }

    #[test]
    fn classify_sunday_over_holiday() {
        // 日曜は法定休日
        let sun = NaiveDate::from_ymd_opt(2026, 1, 4).unwrap();
        assert_eq!(classify_work_day(sun), WorkDayKind::StatutoryHoliday);
    }
}
