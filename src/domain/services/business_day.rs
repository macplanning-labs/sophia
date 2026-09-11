/// domain/services/business_day.rs — 日本の祝日を考慮した営業日計算
///
/// 締め日が土日祝に当たる場合の繰り上げに使う。旧EDI_MP(Django版)の
/// `tasks/scheduler.py::is_business_day`/`_last_business_day` と同じ考え方。

use chrono::{Datelike, NaiveDate, Weekday};

fn to_jpholiday_date(date: NaiveDate) -> jpholiday::Date {
    jpholiday::Date::new(date.year(), date.month(), date.day())
        .expect("chrono::NaiveDate is always a valid calendar date")
}

/// 土日でも祝日でもない日か
pub fn is_business_day(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
        && !jpholiday::is_holiday(to_jpholiday_date(date))
}

/// `date` から遡って `n` 営業日前の日付を返す（`date` 自体は含めない）
pub fn subtract_business_days(date: NaiveDate, n: i32) -> NaiveDate {
    let mut d = date;
    let mut remaining = n;
    while remaining > 0 {
        d -= chrono::Duration::days(1);
        if is_business_day(d) {
            remaining -= 1;
        }
    }
    d
}

/// 締め日が非営業日なら、直前の営業日まで繰り上げる
pub fn roll_to_business_day(date: NaiveDate) -> NaiveDate {
    let mut d = date;
    while !is_business_day(d) {
        d -= chrono::Duration::days(1);
    }
    d
}

/// 締め日が非営業日なら、翌営業日まで繰り下げる
pub fn roll_to_next_business_day(date: NaiveDate) -> NaiveDate {
    let mut d = date;
    while !is_business_day(d) {
        d += chrono::Duration::days(1);
    }
    d
}

// ============================================================
// 案件(m_project)の稼働報告提出期限計算
// ============================================================

/// 稼働報告提出期限の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReportDeadlineType {
    /// 月末からのN営業日前
    Relative,
    /// 当月N日指定
    FixedDay,
}

impl ReportDeadlineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Relative => "RELATIVE",
            Self::FixedDay => "FIXED_DAY",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "FIXED_DAY" => Self::FixedDay,
            _ => Self::Relative,
        }
    }
}

/// FIXED_DAYの基準日が非営業日だった場合の調整ルール
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ReportDeadlineHolidayRule {
    /// 前営業日に前倒し
    PreviousBusinessDay,
    /// 翌営業日に後ろ倒し
    NextBusinessDay,
}

impl ReportDeadlineHolidayRule {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PreviousBusinessDay => "PREVIOUS_BUSINESS_DAY",
            Self::NextBusinessDay => "NEXT_BUSINESS_DAY",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "PREVIOUS_BUSINESS_DAY" => Some(Self::PreviousBusinessDay),
            "NEXT_BUSINESS_DAY" => Some(Self::NextBusinessDay),
            _ => None,
        }
    }
}

/// 案件単位の稼働報告提出期限設定（`m_project.report_deadline_*` に対応）
#[derive(Debug, Clone, Copy)]
pub struct ReportDeadlineSettings {
    pub deadline_type: ReportDeadlineType,
    /// RELATIVE: 月末からのN営業日前（0以上） / FIXED_DAY: 当月N日（1〜月末日）
    pub value: i32,
    /// FIXED_DAYの場合は必須。RELATIVEの場合は無視される
    pub holiday_rule: Option<ReportDeadlineHolidayRule>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReportDeadlineError {
    #[error("無効な年月です: {0}年{1}月")]
    InvalidYearMonth(i32, u32),

    #[error("RELATIVEのvalueは0以上である必要があります: {0}")]
    NegativeRelativeValue(i32),

    #[error("FIXED_DAYのvalueが不正です（1〜{1}の範囲で指定してください）: {0}")]
    DayOutOfRange(i32, u32),

    #[error("FIXED_DAYを指定する場合、holiday_ruleは必須です")]
    MissingHolidayRule,
}

/// 対象年月の稼働報告提出期限を計算する
pub fn calculate_deadline(
    year: i32,
    month: u32,
    settings: &ReportDeadlineSettings,
) -> Result<NaiveDate, ReportDeadlineError> {
    let month_start = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or(ReportDeadlineError::InvalidYearMonth(year, month))?;
    let month_end = last_day_of_month(month_start);

    match settings.deadline_type {
        ReportDeadlineType::Relative => {
            if settings.value < 0 {
                return Err(ReportDeadlineError::NegativeRelativeValue(settings.value));
            }
            Ok(subtract_business_days(month_end, settings.value))
        }
        ReportDeadlineType::FixedDay => {
            let holiday_rule = settings.holiday_rule
                .ok_or(ReportDeadlineError::MissingHolidayRule)?;

            let days_in_month = month_end.day();
            if settings.value < 1 || settings.value as u32 > days_in_month {
                return Err(ReportDeadlineError::DayOutOfRange(settings.value, days_in_month));
            }

            let base = NaiveDate::from_ymd_opt(year, month, settings.value as u32)
                .ok_or(ReportDeadlineError::DayOutOfRange(settings.value, days_in_month))?;

            if is_business_day(base) {
                return Ok(base);
            }

            Ok(match holiday_rule {
                ReportDeadlineHolidayRule::PreviousBusinessDay => roll_to_business_day(base),
                ReportDeadlineHolidayRule::NextBusinessDay => roll_to_next_business_day(base),
            })
        }
    }
}

/// 案件単位の期限設定と契約単位のフォールバック値から、実際に使う`ReportDeadlineSettings`を解決する。
///
/// `project_value`が設定されていれば案件単位の設定（`report_deadline_type`/`value`/`holiday_rule`）を
/// そのまま使う。`None`（案件側で未設定）の場合は、常にRELATIVEとして契約側の
/// `report_deadline_days_before`にフォールバックする。
pub fn resolve_report_deadline_settings(
    project_type: &str,
    project_value: Option<i32>,
    project_holiday_rule: Option<&str>,
    contract_days_before: i32,
) -> ReportDeadlineSettings {
    match project_value {
        Some(value) => ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::from_str(project_type),
            value,
            holiday_rule: project_holiday_rule.and_then(ReportDeadlineHolidayRule::from_str),
        },
        None => ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::Relative,
            value: contract_days_before,
            holiday_rule: None,
        },
    }
}

/// 指定日が属する月の最終日を返す
fn last_day_of_month(month_start: NaiveDate) -> NaiveDate {
    let next_month_start = if month_start.month() == 12 {
        NaiveDate::from_ymd_opt(month_start.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month_start.year(), month_start.month() + 1, 1)
    }
    .expect("year+1/month+1 は常に有効な日付");
    next_month_start.pred_opt().expect("月初の前日は常に存在する")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_new_year_as_holiday() {
        let new_years_day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        assert!(!is_business_day(new_years_day));
    }

    #[test]
    fn recognizes_weekend_as_non_business_day() {
        let saturday = NaiveDate::from_ymd_opt(2026, 7, 11).unwrap();
        assert!(!is_business_day(saturday));
    }

    #[test]
    fn roll_to_business_day_skips_back_over_weekend() {
        let saturday = NaiveDate::from_ymd_opt(2026, 7, 11).unwrap();
        let friday = NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        assert_eq!(roll_to_business_day(saturday), friday);
    }

    #[test]
    fn subtract_business_days_skips_weekend() {
        // 2026/7/13(月) から2営業日前 → 1営業日前は7/10(金)、2営業日前は7/9(木)
        let monday = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
        let thursday = NaiveDate::from_ymd_opt(2026, 7, 9).unwrap();
        assert_eq!(subtract_business_days(monday, 2), thursday);
    }

    #[test]
    fn roll_to_next_business_day_skips_forward_over_weekend() {
        let saturday = NaiveDate::from_ymd_opt(2026, 7, 11).unwrap();
        let monday = NaiveDate::from_ymd_opt(2026, 7, 13).unwrap();
        assert_eq!(roll_to_next_business_day(saturday), monday);
    }

    #[test]
    fn calculate_deadline_relative_counts_back_from_month_end() {
        // 2026年7月の月末は7/31(金)。3営業日前 → 7/30(木)→7/29(水)→7/28(火)
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::Relative,
            value: 3,
            holiday_rule: None,
        };
        let expected = NaiveDate::from_ymd_opt(2026, 7, 28).unwrap();
        assert_eq!(calculate_deadline(2026, 7, &settings), Ok(expected));
    }

    #[test]
    fn calculate_deadline_relative_rejects_negative_value() {
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::Relative,
            value: -1,
            holiday_rule: None,
        };
        assert_eq!(
            calculate_deadline(2026, 7, &settings),
            Err(ReportDeadlineError::NegativeRelativeValue(-1))
        );
    }

    #[test]
    fn calculate_deadline_fixed_day_on_business_day_returns_that_day() {
        // 2026/7/8は水曜・非祝日
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::FixedDay,
            value: 8,
            holiday_rule: Some(ReportDeadlineHolidayRule::NextBusinessDay),
        };
        let expected = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        assert_eq!(calculate_deadline(2026, 7, &settings), Ok(expected));
    }

    #[test]
    fn calculate_deadline_fixed_day_slides_back_over_holiday() {
        // 2026/7/20は海の日（月曜祝日）。前営業日ルールなら7/19(日)→7/18(土)→7/17(金)
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::FixedDay,
            value: 20,
            holiday_rule: Some(ReportDeadlineHolidayRule::PreviousBusinessDay),
        };
        let expected = NaiveDate::from_ymd_opt(2026, 7, 17).unwrap();
        assert_eq!(calculate_deadline(2026, 7, &settings), Ok(expected));
    }

    #[test]
    fn calculate_deadline_fixed_day_slides_forward_over_holiday() {
        // 2026/7/20は海の日（月曜祝日）。翌営業日ルールなら7/21(火)
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::FixedDay,
            value: 20,
            holiday_rule: Some(ReportDeadlineHolidayRule::NextBusinessDay),
        };
        let expected = NaiveDate::from_ymd_opt(2026, 7, 21).unwrap();
        assert_eq!(calculate_deadline(2026, 7, &settings), Ok(expected));
    }

    #[test]
    fn calculate_deadline_fixed_day_requires_holiday_rule() {
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::FixedDay,
            value: 8,
            holiday_rule: None,
        };
        assert_eq!(
            calculate_deadline(2026, 7, &settings),
            Err(ReportDeadlineError::MissingHolidayRule)
        );
    }

    #[test]
    fn calculate_deadline_fixed_day_rejects_day_out_of_month_range() {
        // 2026年4月は30日まで
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::FixedDay,
            value: 31,
            holiday_rule: Some(ReportDeadlineHolidayRule::NextBusinessDay),
        };
        assert_eq!(
            calculate_deadline(2026, 4, &settings),
            Err(ReportDeadlineError::DayOutOfRange(31, 30))
        );
    }

    #[test]
    fn calculate_deadline_rejects_invalid_month() {
        let settings = ReportDeadlineSettings {
            deadline_type: ReportDeadlineType::Relative,
            value: 3,
            holiday_rule: None,
        };
        assert_eq!(
            calculate_deadline(2026, 13, &settings),
            Err(ReportDeadlineError::InvalidYearMonth(2026, 13))
        );
    }

    #[test]
    fn report_deadline_type_round_trips_through_string() {
        assert_eq!(ReportDeadlineType::from_str("FIXED_DAY"), ReportDeadlineType::FixedDay);
        assert_eq!(ReportDeadlineType::from_str("RELATIVE"), ReportDeadlineType::Relative);
        assert_eq!(ReportDeadlineType::from_str("garbage"), ReportDeadlineType::Relative);
        assert_eq!(ReportDeadlineType::FixedDay.as_str(), "FIXED_DAY");
    }

    #[test]
    fn resolve_report_deadline_settings_falls_back_to_contract_when_project_value_unset() {
        let settings = resolve_report_deadline_settings("FIXED_DAY", None, Some("NEXT_BUSINESS_DAY"), 5);
        assert_eq!(settings.deadline_type, ReportDeadlineType::Relative);
        assert_eq!(settings.value, 5);
        assert_eq!(settings.holiday_rule, None);
    }

    #[test]
    fn resolve_report_deadline_settings_uses_project_settings_when_value_set() {
        let settings = resolve_report_deadline_settings("FIXED_DAY", Some(20), Some("NEXT_BUSINESS_DAY"), 5);
        assert_eq!(settings.deadline_type, ReportDeadlineType::FixedDay);
        assert_eq!(settings.value, 20);
        assert_eq!(settings.holiday_rule, Some(ReportDeadlineHolidayRule::NextBusinessDay));
    }

    #[test]
    fn resolve_report_deadline_settings_treats_blank_holiday_rule_as_none() {
        let settings = resolve_report_deadline_settings("RELATIVE", Some(3), Some(""), 5);
        assert_eq!(settings.deadline_type, ReportDeadlineType::Relative);
        assert_eq!(settings.value, 3);
        assert_eq!(settings.holiday_rule, None);
    }
}
