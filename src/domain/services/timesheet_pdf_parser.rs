/// domain/services/timesheet_pdf_parser.rs — 稼働報告（勤務表）PDF パーサ
///
/// 現状はクロスシステムサービス株式会社の定型「勤務表」PDF（テキスト埋め込み）向け。
/// スキャン画像PDFや他社フォーマットは対象外（OCRは未対応）。
///
/// 出力は Excel パーサと同じ `TimesheetParseResult` で、既存のプレビュー→確認登録フローに乗せる。

use anyhow::{Result, bail};
use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;
use tracing::warn;

use super::excel_parser::{
    check_work_alerts, DailyEntry, TimesheetParseResult, error_result_for_pdf,
};

/// PDFバイト列を解析する（拡張子判定は呼び出し側 / `excel_parser::auto_detect_and_parse`）
pub fn parse(file_bytes: &[u8], original_filename: &str) -> TimesheetParseResult {
    match parse_inner(file_bytes, original_filename) {
        Ok(result) => result,
        Err(e) => {
            warn!("勤務表PDF解析エラー: {}", e);
            error_result_for_pdf(&format!("PDFの解析中にエラーが発生しました: {e}"))
        }
    }
}

fn parse_inner(file_bytes: &[u8], original_filename: &str) -> Result<TimesheetParseResult> {
    let text = pdf_extract::extract_text_from_mem(file_bytes)
        .map_err(|e| anyhow::anyhow!("PDFテキスト抽出失敗: {e}"))?;
    parse_cross_timesheet_text(&text, original_filename)
}

/// テキスト抽出済みのクロス勤務表をパース（単体テスト用に公開）
pub fn parse_cross_timesheet_text(text: &str, original_filename: &str) -> Result<TimesheetParseResult> {
    if !looks_like_cross_timesheet(text, original_filename) {
        bail!("対応していない勤務表PDFです（クロスシステムの定型勤務表のみ対応）");
    }

    let (year, month) = extract_year_month(text, original_filename)?
        .ok_or_else(|| anyhow::anyhow!("稼動年月を特定できませんでした"))?;

    let worker_name = extract_worker_name(text, original_filename)?;
    if worker_name.is_empty() {
        bail!("氏名を特定できませんでした");
    }

    let daily_data = extract_daily_entries(text, year, month)?;
    if daily_data.iter().all(|d| d.hours <= 0.0) {
        bail!("日別の稼働時間が抽出できませんでした");
    }

    let total_from_footer = extract_total_hours(text);
    let sum_hours: f64 = daily_data.iter().map(|d| d.hours).sum();
    let total_hours = total_from_footer.unwrap_or(sum_hours);
    // フッター合計と日次合算が大きくずれる場合は日次を優先（抽出崩れの検知）
    let total_hours = if total_from_footer.is_some() && (total_hours - sum_hours).abs() > 0.26 {
        warn!(
            "勤務表PDF: 合計({})と日次合算({})が不一致のため日次合算を採用",
            total_hours, sum_hours
        );
        sum_hours
    } else {
        total_hours
    };

    let work_days = daily_data.iter().filter(|d| d.hours > 0.0).count() as i32;
    let total_dec = Decimal::from_f64_retain(total_hours)
        .unwrap_or(Decimal::ZERO)
        .round_dp(2);

    let alerts = check_work_alerts(&daily_data);
    let has_times = daily_data.iter().any(|d| !d.start.is_empty() && !d.end.is_empty());

    Ok(TimesheetParseResult {
        worker_name,
        target_month: NaiveDate::from_ymd_opt(year, month, 1),
        total_hours: total_dec,
        work_days,
        overtime_hours: Decimal::ZERO,
        night_hours: Decimal::ZERO,
        holiday_hours: Decimal::ZERO,
        daily_data,
        alerts,
        sheet_name: "勤務表(PDF)".to_string(),
        error: None,
        has_times,
    })
}

fn looks_like_cross_timesheet(text: &str, filename: &str) -> bool {
    let has_title = text.contains("勤務表") || filename.contains("勤務表");
    let has_marker = text.contains("稼動年月")
        || text.contains("実稼働時間")
        || text.contains("クロスシステム");
    has_title && has_marker
}

fn extract_year_month(text: &str, filename: &str) -> Result<Option<(i32, u32)>> {
    let re = Regex::new(r"(\d{4})\s*年\s*(\d{1,2})\s*月").unwrap();
    if let Some(c) = re.captures(text) {
        let y: i32 = c[1].parse()?;
        let m: u32 = c[2].parse()?;
        if (1..=12).contains(&m) {
            return Ok(Some((y, m)));
        }
    }
    // ファイル名: 勤務表_2026年07月... or 勤務表_2026年7月...
    let re_fn = Regex::new(r"(\d{4})\s*年\s*(\d{1,2})\s*月").unwrap();
    if let Some(c) = re_fn.captures(filename) {
        let y: i32 = c[1].parse()?;
        let m: u32 = c[2].parse()?;
        if (1..=12).contains(&m) {
            return Ok(Some((y, m)));
        }
    }
    Ok(None)
}

fn normalize_person_name(raw: &str) -> String {
    raw.replace('\u{3000}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn extract_worker_name(text: &str, filename: &str) -> Result<String> {
    // 「氏名 xxx」同一行
    let re_same = Regex::new(r"氏名[：:\s　]*([^\n\r]+)").unwrap();
    if let Some(c) = re_same.captures(text) {
        let mut name = normalize_person_name(c[1].trim());
        // 後続ラベルがつながった場合を除去
        for stop in ["会社名", "案件名", "稼動年月", "実稼働", "報告者", "承認者"] {
            if let Some(idx) = name.find(stop) {
                name = name[..idx].trim().to_string();
            }
        }
        if !name.is_empty() && name != "氏名" {
            return Ok(name);
        }
    }

    // 行単位: 氏名\n勝又　祐紀
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    for i in 0..lines.len() {
        if lines[i] == "氏名" || lines[i].starts_with("氏名") {
            if let Some(next) = lines.get(i + 1) {
                let name = normalize_person_name(next);
                if !name.is_empty()
                    && !name.contains("会社")
                    && !name.contains("案件")
                    && name != "稼動年月"
                {
                    return Ok(name);
                }
            }
        }
    }

    // ファイル名: 勤務表_2026年07月21日_勝又祐紀.pdf
    if let Some(stem) = filename
        .rsplit('/')
        .next()
        .and_then(|f| f.strip_suffix(".pdf").or_else(|| f.strip_suffix(".PDF")))
    {
        if let Some((_, name_part)) = stem.rsplit_once('_') {
            let name = normalize_person_name(name_part);
            if !name.is_empty() && !Regex::new(r"^\d").unwrap().is_match(&name) {
                return Ok(name);
            }
        }
    }

    Ok(String::new())
}

fn extract_total_hours(text: &str) -> Option<f64> {
    let re = Regex::new(r"合計\s*(\d+)\s*時間\s*(\d+)\s*分").unwrap();
    re.captures(text).map(|c| {
        let h: f64 = c[1].parse().unwrap_or(0.0);
        let m: f64 = c[2].parse().unwrap_or(0.0);
        h + m / 60.0
    })
}

fn extract_daily_entries(text: &str, year: i32, month: u32) -> Result<Vec<DailyEntry>> {
    // pypdf系: "1 水 09:00 18:00 60 8 時間 0 分"
    let re_work = Regex::new(
        r"(?m)^(\d{1,2})\s+([月火水木金土日祝])\s+(\d{2}:\d{2})\s+(\d{2}:\d{2})\s+(\d+)\s+(\d+)\s*時間\s+(\d+)\s*分",
    )
    .unwrap();
    // 休: "4 土" / "20 祝"
    let re_off = Regex::new(r"(?m)^(\d{1,2})\s+([月火水木金土日祝])\s*$").unwrap();

    let mut by_day: std::collections::BTreeMap<u32, DailyEntry> = std::collections::BTreeMap::new();

    for c in re_work.captures_iter(text) {
        let day: u32 = c[1].parse()?;
        let day_name = c[2].to_string();
        let start = c[3].to_string();
        let end = c[4].to_string();
        let hours: f64 = c[6].parse::<f64>()? + c[7].parse::<f64>()? / 60.0;
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        by_day.insert(
            day,
            DailyEntry {
                date,
                day_name,
                hours: (hours * 100.0).round() / 100.0,
                start,
                end,
            },
        );
    }

    for c in re_off.captures_iter(text) {
        let day: u32 = c[1].parse()?;
        if by_day.contains_key(&day) {
            continue;
        }
        let day_name = c[2].to_string();
        let date = NaiveDate::from_ymd_opt(year, month, day)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        by_day.insert(
            day,
            DailyEntry {
                date,
                day_name,
                hours: 0.0,
                start: String::new(),
                end: String::new(),
            },
        );
    }

    // pdf-extract が列単位で崩す場合のフォールバック:
    // 日付行・開始時刻列・終了時刻列・「N 時間 M 分」を別々に拾って突き合わせる
    if by_day.values().filter(|d| d.hours > 0.0).count() < 5 {
        if let Some(entries) = extract_daily_columnar(text, year, month) {
            for (day, entry) in entries {
                by_day.insert(day, entry);
            }
        }
    }

    if by_day.is_empty() {
        bail!("日付行を抽出できませんでした");
    }

    Ok(by_day.into_values().collect())
}

/// pdfminer/pdf-extract 風の列バラバラテキスト向け
fn extract_daily_columnar(text: &str, year: i32, month: u32) -> Option<std::collections::BTreeMap<u32, DailyEntry>> {
    let day_re = Regex::new(r"(?m)^(\d{1,2})\s+([月火水木金土日祝])\s*$").unwrap();
    let time_re = Regex::new(r"(?m)^(\d{2}:\d{2})$").unwrap();
    let hours_re = Regex::new(r"(?m)^(\d+)\s*時間(?:\s*(\d+)\s*分)?").unwrap();

    let days: Vec<(u32, String)> = day_re
        .captures_iter(text)
        .filter_map(|c| Some((c[1].parse().ok()?, c[2].to_string())))
        .collect();
    let times: Vec<String> = time_re
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect();
    let hours_list: Vec<f64> = hours_re
        .captures_iter(text)
        .filter_map(|c| {
            // 「合計 N 時間」はスキップしたい — 直前コンテキストは見ず、大きすぎる値を除外
            let h: f64 = c[1].parse().ok()?;
            let m: f64 = c.get(2).and_then(|m| m.as_str().parse().ok()).unwrap_or(0.0);
            let v = h + m / 60.0;
            if v > 24.0 {
                None
            } else {
                Some(v)
            }
        })
        .collect();

    // 開始・終了がペアで times に並ぶ想定（出勤日数×2）
    let work_days: Vec<(u32, String)> = days
        .iter()
        .cloned()
        .filter(|(_, dn)| !matches!(dn.as_str(), "土" | "日" | "祝"))
        .collect();

    if work_days.is_empty() || times.len() < work_days.len() * 2 {
        return None;
    }
    // hours が出勤日数と一致しない場合は諦める
    if hours_list.len() < work_days.len() {
        return None;
    }

    let mut map = std::collections::BTreeMap::new();
    for (i, (day, day_name)) in work_days.iter().enumerate() {
        let start = times.get(i).cloned().unwrap_or_default();
        let end = times.get(work_days.len() + i).cloned().unwrap_or_default();
        let hours = hours_list.get(i).copied().unwrap_or(0.0);
        let date = NaiveDate::from_ymd_opt(year, month, *day)?
            .format("%Y-%m-%d")
            .to_string();
        map.insert(
            *day,
            DailyEntry {
                date,
                day_name: day_name.clone(),
                hours: (hours * 100.0).round() / 100.0,
                start,
                end,
            },
        );
    }

    // 休みの日も埋める
    for (day, day_name) in days {
        map.entry(day).or_insert_with(|| {
            let date = NaiveDate::from_ymd_opt(year, month, day)
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            DailyEntry {
                date,
                day_name,
                hours: 0.0,
                start: String::new(),
                end: String::new(),
            }
        });
    }

    Some(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::prelude::ToPrimitive;

    #[test]
    fn parses_cross_timesheet_text_fixture() {
        let text = std::fs::read_to_string("document/勤務表_クロスシステム_サンプル.txt").unwrap();
        let result = parse_cross_timesheet_text(
            &text,
            "勤務表_2026年07月21日_勝又祐紀.pdf",
        )
        .unwrap();

        assert!(result.error.is_none());
        assert_eq!(result.target_month, NaiveDate::from_ymd_opt(2026, 7, 1));
        assert_eq!(result.worker_name, "勝又 祐紀");
        assert_eq!(result.work_days, 22);
        assert!((result.total_hours.to_f64().unwrap() - 198.5).abs() < 0.01);
        assert!(result.has_times);
        assert!(result.daily_data.iter().any(|d| d.date == "2026-07-01" && d.hours == 8.0));
        assert!(result.daily_data.iter().any(|d| d.date == "2026-07-15" && (d.hours - 10.5).abs() < 0.01));
    }

    #[test]
    fn parses_cross_timesheet_pdf_bytes() {
        let bytes = std::fs::read("document/勤務表_クロスシステム_サンプル.pdf").unwrap();
        let result = parse(&bytes, "勤務表_2026年07月21日_勝又祐紀.pdf");
        assert!(
            result.error.is_none(),
            "PDF解析エラー: {:?}",
            result.error
        );
        assert_eq!(result.target_month, NaiveDate::from_ymd_opt(2026, 7, 1));
        assert_eq!(result.work_days, 22);
        assert!((result.total_hours.to_f64().unwrap_or(0.0) - 198.5).abs() < 0.01);
        // 氏名はスペース正規化済み（全角スペース→半角）
        assert!(result.worker_name.contains("勝又"));
        assert!(result.worker_name.contains("祐紀"));
    }
}
