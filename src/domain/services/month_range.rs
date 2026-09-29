//! 年月パラメータの安全な処理
//!
//! `daily_work_entry/work.rs` の年月パラメータが `i32` で無検証の場合、
//! `from_ymd_opt(..).unwrap()` でパニックする問題を解消する。
//! 無効な年月（month=0/13/-1等）の場合は None を返す。

use chrono::{Datelike, Months, NaiveDate};

/// 年月（月初〜翌月初）の範囲を返す。範囲外の年月なら None。
///
/// # Example
/// ```
/// # use sophia::domain::services::month_range::month_range;
/// # use chrono::NaiveDate;
/// let (start, end) = month_range(2026, 1).unwrap();
/// assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
/// assert_eq!(end, NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
/// ```
pub fn month_range(year: i32, month: i32) -> Option<(NaiveDate, NaiveDate)> {
    // month は 1..=12 のみ有効
    if !(1..=12).contains(&month) {
        return None;
    }

    // 開始日: year/month/1
    let start = NaiveDate::from_ymd_opt(year, month as u32, 1)?;

    // 終了日: 翌月初
    let end = start.checked_add_months(Months::new(1))?;

    Some((start, end))
}

/// 月初日から翌月初日。年またぎ・オーバーフロー安全。
///
/// # Example
/// ```
/// # use sophia::domain::services::month_range::first_of_next_month;
/// # use chrono::NaiveDate;
/// // 通常月
/// let oct_1 = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
/// let nov_1 = first_of_next_month(oct_1).unwrap();
/// assert_eq!(nov_1, NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
///
/// // 12月は翌年1月1日
/// let dec_31 = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
/// let jan_1 = first_of_next_month(dec_31).unwrap();
/// assert_eq!(jan_1, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
/// ```
pub fn first_of_next_month(date: NaiveDate) -> Option<NaiveDate> {
    // 月初に戻す
    let month_start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1)?;
    // 翌月初を取得
    month_start.checked_add_months(Months::new(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_month_range_normal_month() {
        let (start, end) = month_range(2026, 5).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap());
    }

    #[test]
    fn test_month_range_december() {
        let (start, end) = month_range(2026, 12).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }

    #[test]
    fn test_month_range_leap_year_february() {
        // 2024年はうるう年（2月29日が存在）
        let (start, end) = month_range(2024, 2).unwrap();
        assert_eq!(start, NaiveDate::from_ymd_opt(2024, 2, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2024, 3, 1).unwrap());
    }

    #[test]
    fn test_month_range_invalid_month_zero() {
        assert_eq!(month_range(2026, 0), None);
    }

    #[test]
    fn test_month_range_invalid_month_thirteen() {
        assert_eq!(month_range(2026, 13), None);
    }

    #[test]
    fn test_month_range_invalid_month_negative() {
        assert_eq!(month_range(2026, -1), None);
    }

    #[test]
    fn test_month_range_year_max() {
        // i32::MAX = 2147483647（year が大きすぎる場合）
        // checked_add_months が失敗するはず
        assert_eq!(month_range(i32::MAX, 1), None);
    }

    #[test]
    fn test_month_range_year_min() {
        // i32::MIN でも同じ
        assert_eq!(month_range(i32::MIN, 1), None);
    }

    #[test]
    fn test_first_of_next_month_normal() {
        let oct_15 = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let nov_1 = first_of_next_month(oct_15).unwrap();
        assert_eq!(nov_1, NaiveDate::from_ymd_opt(2026, 11, 1).unwrap());
    }

    #[test]
    fn test_first_of_next_month_december() {
        let dec_31 = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        let jan_1 = first_of_next_month(dec_31).unwrap();
        assert_eq!(jan_1, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }

    #[test]
    fn test_first_of_next_month_leap_year() {
        let feb_29 = NaiveDate::from_ymd_opt(2024, 2, 29).unwrap();
        let mar_1 = first_of_next_month(feb_29).unwrap();
        assert_eq!(mar_1, NaiveDate::from_ymd_opt(2024, 3, 1).unwrap());
    }

    #[test]
    fn test_first_of_next_month_year_boundary() {
        let jan_1 = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let feb_1 = first_of_next_month(jan_1).unwrap();
        assert_eq!(feb_1, NaiveDate::from_ymd_opt(2026, 2, 1).unwrap());
    }

    #[test]
    fn test_first_of_next_month_overflow() {
        // NaiveDate::MAX の次の月は None
        let max_date = NaiveDate::MAX;
        // MAX は 262144-12-31 なので、翌月を取ろうとするとオーバーフロー
        assert_eq!(first_of_next_month(max_date), None);
    }
}
