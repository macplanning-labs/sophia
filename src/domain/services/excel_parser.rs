/// domain/services/excel_parser.rs — 稼働報告書 Excel 自動読取りサービス
///
/// EDI_MP の `excel_parser.py`（659行・8ステップ解析）を calamine crate で移植。
///
/// ## 解析ステップ
/// 1. 稼働報告シートの特定（サンプル/祝日シートは除外）
/// 2. 日付列の検出（A〜E列をスキャン）
/// 3. 作業時間列の検出（分単位 or 時間単位を自動判別）
/// 4. 日別データの抽出
/// 5. 作業月の検出（日付データ → ファイル名のフォールバック）
/// 6. 合計時間・稼働日数の検出
/// 7. 作業者名の取得（ファイル名のフォールバック）
/// 8. 土日祝・稼働チェック（15分単位含む）

use anyhow::{Result, bail};
use calamine::{Reader, Xlsx, Data, open_workbook_from_rs};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Cursor;
use tracing::{info, warn};

/// シート名に含まれていたら除外するキーワード
const IGNORE_SHEET_KEYWORDS: &[&str] = &[
    "サンプル", "テスト", "sample", "test", "祝日", "holiday", "template", "テンプレ",
];

/// 曜日名
const DAY_NAMES: &[&str] = &["月", "火", "水", "木", "金", "土", "日"];

// ── 公開API ──

/// Excel解析結果
#[derive(Debug, Clone, Serialize)]
pub struct TimesheetParseResult {
    pub worker_name: String,
    /// 勤務表に書かれた案件名（読み取れないときは空）。受注との照合の補助に使う
    pub project_name: String,
    pub target_month: Option<NaiveDate>,
    pub total_hours: Decimal,
    pub work_days: i32,
    pub overtime_hours: Decimal,
    pub night_hours: Decimal,
    pub holiday_hours: Decimal,
    pub daily_data: Vec<DailyEntry>,
    pub alerts: Vec<WorkAlert>,
    pub sheet_name: String,
    pub error: Option<String>,
    /// 開始・終了時刻データがあるか
    pub has_times: bool,
}

/// 日別エントリ
#[derive(Debug, Clone, Default, Serialize)]
pub struct DailyEntry {
    pub date: String,
    pub day_name: String,
    /// 実働時間。勤務表PDFでは、開始・終了・休憩からアプリが計算した値
    pub hours: f64,
    pub start: String,
    pub end: String,
    /// 勤務表に書かれた休憩（分）。読み取れない書式では None
    pub break_minutes: Option<i32>,
    /// 勤務表に書かれた実働時間（比較用）。アプリの計算値と合わない日に警告する。読み取れない書式では None
    pub stated_hours: Option<f64>,
}

/// 稼働チェック警告
#[derive(Debug, Clone, Serialize)]
pub struct WorkAlert {
    pub date: String,
    pub alert_type: String,
    pub hours: f64,
    pub day_name: String,
    pub detail: String,
}

/// Excelファイルを自動判別してパースする。
/// 拡張子が `.pdf`、またはマジックバイトが `%PDF` の場合は勤務表PDFパーサへディスパッチする。
/// multipart で `file_name` が欠落／誤拡張子でも、PDF本体なら Excel(ZIP) 経路に落とさない。
pub fn auto_detect_and_parse(
    file_bytes: &[u8],
    original_filename: &str,
) -> TimesheetParseResult {
    let lower = original_filename.to_ascii_lowercase();
    let is_pdf_ext = lower.ends_with(".pdf");
    let is_pdf_magic = file_bytes.len() >= 4 && file_bytes.starts_with(b"%PDF");
    if is_pdf_ext || is_pdf_magic {
        // PDF の勤務表は取引先ごとの様式なので、カスタマイズに任せる（無ければ「対応する様式なし」）
        return crate::custom::parse_timesheet_pdf(file_bytes, original_filename);
    }
    match parse_inner(file_bytes, original_filename) {
        Ok(result) => result,
        Err(e) => {
            warn!("Excel解析エラー: {}", e);
            error_result(&format!("ファイルの解析中にエラーが発生しました: {}", e))
        }
    }
}

fn error_result(message: &str) -> TimesheetParseResult {
    TimesheetParseResult {
        worker_name: String::new(),
        project_name: String::new(),
        target_month: None,
        total_hours: Decimal::ZERO,
        work_days: 0,
        overtime_hours: Decimal::ZERO,
        night_hours: Decimal::ZERO,
        holiday_hours: Decimal::ZERO,
        daily_data: vec![],
        alerts: vec![],
        sheet_name: String::new(),
        error: Some(message.to_string()),
        has_times: false,
    }
}

/// PDFパーサ等から共通のエラー結果を返す
pub(crate) fn error_result_for_pdf(message: &str) -> TimesheetParseResult {
    error_result(message)
}

fn parse_inner(file_bytes: &[u8], original_filename: &str) -> Result<TimesheetParseResult> {
    let cursor = Cursor::new(file_bytes);
    let mut workbook: Xlsx<_> = open_workbook_from_rs(cursor)?;

    let sheet_names: Vec<String> = workbook.sheet_names().to_vec();

    // Step 1: シート検出
    let (sheet_name, sheet_data) = detect_work_sheet(&mut workbook, &sheet_names)?;
    info!("[Excel] 検出シート: {}", sheet_name);

    // Step 1.5: ヘッダー情報検出（「稼動年月」「氏名」ラベルセルからの相対位置）
    // 例: ある取引先の勤務表フォーマットは A3="稼動年月" D3=西暦 E3=月、F3="氏名" I3=氏名の値
    // という配置（ラベルセルから見て west+3/+4列目に値がある）。この年月が検出できた場合、
    // 日付列の値が1〜31の小さい整数（=Excelシリアル値ではなく「日」のみの入力）であっても
    // 正しい日付を組み立てられるようにする。
    let header_info = detect_header_info(&sheet_data);
    if let Some((y, m)) = header_info.year_month {
        info!("[Excel] ヘッダーから稼動年月を検出: {}-{:02}", y, m);
    }

    // Step 2: 日付列検出
    let (date_col, data_rows) = detect_date_column(&sheet_data);
    if data_rows.len() < 20 {
        bail!("日付データの列を検出できませんでした（{}行しか見つかりません）", data_rows.len());
    }

    // Step 3: 作業時間列検出
    let (hours_col, hours_unit) = detect_hours_column(&sheet_data, date_col, &data_rows);
    let Some(hours_col) = hours_col else {
        bail!("作業時間データの列を検出できませんでした");
    };

    // Step 3.5: 開始・終了時刻列検出
    let time_cols = detect_time_columns(&sheet_data, date_col, hours_col, &data_rows);
    if time_cols.has_times() {
        info!("[Excel] 時刻列検出: {:?}", time_cols);
    }

    // Step 4: 日別データ抽出
    let daily_data = extract_daily_data(&sheet_data, date_col, hours_col, &hours_unit, &data_rows, &time_cols, header_info.year_month);

    // Step 5: 作業月検出
    let target_month = detect_target_month(&daily_data, original_filename);

    // Step 6: 合計・稼働日数
    let (total_hours, work_days) = detect_totals(&sheet_data, &daily_data);

    // Step 7: 作業者名（シート内の「氏名」セルを優先、無ければファイル名から抽出）
    let worker_name = header_info.worker_name
        .unwrap_or_else(|| extract_worker_name_from_filename(original_filename));

    // Step 8: 稼働チェック
    let alerts = check_work_alerts(&daily_data);

    // 精算超過・深夜・休日は受注契約確定後に upload_common で設定する（法定残業は表示しない）
    Ok(TimesheetParseResult {
        worker_name,
        project_name: detect_project_name(&sheet_data),
        target_month,
        total_hours,
        work_days,
        overtime_hours: Decimal::ZERO,
        night_hours: Decimal::ZERO,
        holiday_hours: Decimal::ZERO,
        daily_data,
        alerts,
        sheet_name,
        error: None,
        has_times: time_cols.has_times(),
    })
}

// ── Step 1.5: ヘッダー情報検出（稼動年月・氏名）──

struct HeaderInfo {
    year_month: Option<(i32, u32)>,
    worker_name: Option<String>,
}

/// 「稼動年月」「氏名」ラベルセルを探し、その相対位置（+3列/+4列）から値を読み取る。
/// 取引先の勤務表フォーマット（A3="稼動年月" D3=西暦 E3=月、F3="氏名" I3=氏名）で
/// 確認された配置。ラベルセルからの相対オフセットにしているため、この行全体が別の列位置に
/// ずれても追従できる。
fn detect_header_info(rows: &[Vec<Data>]) -> HeaderInfo {
    let mut year_month = None;
    let mut worker_name = None;

    for row in rows.iter().take(10) {
        for (col_idx, cell) in row.iter().enumerate() {
            let Data::String(label) = cell else { continue };
            let label = label.trim();

            if year_month.is_none() && (label == "稼動年月" || label == "対象年月" || label == "作業年月") {
                let year = row.get(col_idx + 3).and_then(cell_to_f64).map(|v| v as i32);
                let month = row.get(col_idx + 4).and_then(cell_to_f64).map(|v| v as u32);
                if let (Some(y), Some(m)) = (year, month) {
                    if (1..=12).contains(&m) && (2000..=2100).contains(&y) {
                        year_month = Some((y, m));
                    }
                }
            }

            if worker_name.is_none() && label == "氏名" {
                if let Some(Data::String(name)) = row.get(col_idx + 3) {
                    let name = name.trim();
                    if !name.is_empty() {
                        worker_name = Some(name.to_string());
                    }
                }
            }
        }
    }

    HeaderInfo { year_month, worker_name }
}

// ── Step 1: シート検出 ──

fn detect_work_sheet(
    workbook: &mut Xlsx<Cursor<&[u8]>>,
    sheet_names: &[String],
) -> Result<(String, Vec<Vec<Data>>)> {
    let mut candidates: Vec<(i32, String)> = Vec::new();

    for name in sheet_names {
        let lower = name.to_lowercase();
        if IGNORE_SHEET_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
            continue;
        }

        if let Ok(range) = workbook.worksheet_range(name) {
            let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
            let score = score_sheet(&rows);
            if score > 0 {
                candidates.push((score, name.clone()));
            }
        }
    }

    // フォールバック: 除外対象も含めて再チェック
    if candidates.is_empty() {
        for name in sheet_names {
            if let Ok(range) = workbook.worksheet_range(name) {
                let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
                let score = score_sheet(&rows);
                if score > 0 {
                    candidates.push((score, name.clone()));
                }
            }
        }
    }

    candidates.sort_by(|a, b| b.0.cmp(&a.0));

    if let Some((_, best_name)) = candidates.first() {
        let range = workbook.worksheet_range(best_name)?;
        let rows: Vec<Vec<Data>> = range.rows().map(|r| r.to_vec()).collect();
        Ok((best_name.clone(), rows))
    } else {
        bail!("稼働報告データのあるシートが見つかりませんでした");
    }
}

fn score_sheet(rows: &[Vec<Data>]) -> i32 {
    // 日付列がA〜Eのどこにあるかはシートによって異なる（calamineはシートの実使用範囲だけを
    // 返すため、例えばB列から使用されているシートでは配列のindex 0が実際のB列に対応する）。
    // Step2の detect_date_column と同じくA〜E列を走査し、最も日付らしい列の件数を採用する。
    let mut date_count = 0i32;
    for col_idx in 0..5 {
        let mut count = 0i32;
        for row in rows.iter().take(44) {
            if row.len() > col_idx && is_date_value(&row[col_idx]) {
                count += 1;
            }
        }
        if count > date_count {
            date_count = count;
        }
    }

    if date_count >= 20 {
        date_count
    } else if date_count >= 10 {
        date_count / 2
    } else {
        0
    }
}

fn is_date_value(cell: &Data) -> bool {
    match cell {
        Data::DateTime(_) | Data::DateTimeIso(_) => true,
        Data::Float(f) => {
            // Excelのシリアル日付値、または日付が「日」の数値のみで入力された値
            // （1〜80000程度、整数値のみ）。">1.0"という厳密不等号だと月初日(1)の
            // データが除外されてしまうため">=1.0"とする。
            *f >= 1.0 && *f < 80000.0 && *f == f.floor()
        }
        _ => false,
    }
}

// ── Step 2: 日付列検出 ──

fn detect_date_column(rows: &[Vec<Data>]) -> (usize, Vec<usize>) {
    let mut best_col = 1usize; // デフォルトB列
    let mut best_rows: Vec<usize> = Vec::new();
    let mut best_count = 0usize;

    // A〜E列をチェック
    for col_idx in 0..5 {
        let mut found_rows = Vec::new();
        for (row_idx, row) in rows.iter().enumerate().take(44) {
            if row.len() > col_idx && is_date_value(&row[col_idx]) {
                found_rows.push(row_idx);
            }
        }
        if found_rows.len() > best_count {
            best_count = found_rows.len();
            best_col = col_idx;
            best_rows = found_rows;
        }
    }

    (best_col, best_rows)
}

// ── Step 3: 作業時間列検出 ──

fn detect_hours_column(
    rows: &[Vec<Data>],
    date_col: usize,
    data_rows: &[usize],
) -> (Option<usize>, String) {
    let mut candidates: Vec<(i32, usize, String)> = Vec::new();

    let max_col = rows.iter().map(|r| r.len()).max().unwrap_or(0).min(25);

    for col_idx in (date_col + 1)..max_col {
        let mut values: Vec<f64> = Vec::new();
        for &row_idx in data_rows {
            if let Some(row) = rows.get(row_idx) {
                if let Some(cell) = row.get(col_idx) {
                    if let Some(v) = cell_to_f64(cell) {
                        if v > 0.0 {
                            values.push(v);
                        }
                    }
                }
            }
        }

        if values.is_empty() {
            continue;
        }

        let max_val = values.iter().cloned().fold(0.0f64, f64::max);
        let avg_val = values.iter().sum::<f64>() / values.len() as f64;

        // 分単位（60〜1440の範囲）
        if max_val <= 1440.0 && avg_val >= 60.0 {
            let score = values.len() as i32;
            candidates.push((score, col_idx, "minutes".to_string()));
        }
        // 時間単位（0〜24の範囲）
        else if max_val <= 24.0 && avg_val >= 1.0 {
            let score = values.len() as i32;
            candidates.push((score, col_idx, "hours".to_string()));
        }
    }

    // 分単位の列を優先
    let minute_candidates: Vec<_> = candidates.iter().filter(|c| c.2 == "minutes").collect();
    if let Some(best) = minute_candidates.iter().max_by_key(|c| c.0) {
        return (Some(best.1), "minutes".to_string());
    }

    if let Some(best) = candidates.iter().max_by_key(|c| c.0) {
        return (Some(best.1), best.2.clone());
    }

    (None, String::new())
}

/// 「案件名」の見出しセルの、右にある最初の文字列セルを、案件名として返す（無ければ空）
fn detect_project_name(rows: &[Vec<Data>]) -> String {
    for row in rows.iter().take(15) {
        for (i, cell) in row.iter().enumerate() {
            let Data::String(label) = cell else { continue };
            let label: String = label.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect();
            if label == "案件名" {
                return row
                    .iter()
                    .skip(i + 1)
                    .find_map(|c| match c {
                        Data::String(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
                        _ => None,
                    })
                    .unwrap_or_default();
            }
        }
    }
    String::new()
}

/// "09:00" → 分。読めなければ None
fn hhmm_to_minutes(t: &str) -> Option<i32> {
    let (h, m) = t.split_once(':')?;
    Some(h.trim().parse::<i32>().ok()? * 60 + m.trim().parse::<i32>().ok()?)
}

/// 開始・終了・休憩（分）から実働時間（時間）を計算する。
/// 終了が開始以前、休憩を引くと負になる、時刻が読めない場合は None（呼び出し側は勤務表の記載を使う）
pub(crate) fn compute_work_hours(start: &str, end: &str, break_minutes: i32) -> Option<f64> {
    let (s, e) = (hhmm_to_minutes(start)?, hhmm_to_minutes(end)?);
    let worked = e - s - break_minutes;
    if e <= s || worked < 0 {
        return None;
    }
    Some(f64::from(worked) / 60.0)
}

// ── Step 4: 日別データ抽出 ──

fn extract_daily_data(
    rows: &[Vec<Data>],
    date_col: usize,
    hours_col: usize,
    hours_unit: &str,
    data_rows: &[usize],
    time_cols: &TimeCols,
    year_month: Option<(i32, u32)>,
) -> Vec<DailyEntry> {
    let mut daily_data = Vec::new();

    for &row_idx in data_rows {
        let row = match rows.get(row_idx) {
            Some(r) => r,
            None => continue,
        };

        let date_val = match row.get(date_col) {
            Some(cell) => cell_to_date_with_ym(cell, year_month),
            None => continue,
        };

        let d = match date_val {
            Some(d) => d,
            None => continue,
        };

        let hours_raw = row.get(hours_col).and_then(cell_to_f64).unwrap_or(0.0);
        let hours = if hours_unit == "minutes" {
            (hours_raw / 60.0 * 100.0).round() / 100.0
        } else {
            (hours_raw * 100.0).round() / 100.0
        };

        let weekday = d.weekday().num_days_from_monday() as usize;
        let day_name = DAY_NAMES.get(weekday).unwrap_or(&"").to_string();

        let start = time_cols.read_start(row);
        let end = time_cols.read_end(row);
        let break_minutes = time_cols.read_break(row);

        // 開始・終了・休憩の列をヘッダーで確実に見つけた書式は、実働時間をアプリで計算する
        // （勤務表に書かれた実働時間は、比較用に stated_hours へ残す）
        let computed = if time_cols.by_header {
            break_minutes.and_then(|b| compute_work_hours(&start, &end, b))
        } else {
            None
        };
        let (hours, stated_hours) = match computed {
            Some(c) => ((c * 100.0).round() / 100.0, Some(hours)),
            None => (hours, None),
        };

        daily_data.push(DailyEntry {
            date: d.format("%Y-%m-%d").to_string(),
            day_name,
            hours,
            start,
            end,
            break_minutes,
            stated_hours,
        });
    }

    daily_data
}

// ── Step 5: 作業月検出 ──

fn detect_target_month(
    daily_data: &[DailyEntry],
    filename: &str,
) -> Option<NaiveDate> {
    // 1. 日別データから月を推定
    if !daily_data.is_empty() {
        let mut month_counts: HashMap<(i32, u32), usize> = HashMap::new();
        for entry in daily_data {
            if let Ok(d) = NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
                *month_counts.entry((d.year(), d.month())).or_insert(0) += 1;
            }
        }
        if let Some(((year, month), _)) = month_counts.iter().max_by_key(|e| e.1) {
            return NaiveDate::from_ymd_opt(*year, *month, 1);
        }
    }

    // 2. ファイル名から YYYY年MM or YYYY/MM or YYYY-MM
    let re = regex::Regex::new(r"(\d{4})[年/\-](\d{1,2})").ok()?;
    if let Some(caps) = re.captures(filename) {
        let year: i32 = caps[1].parse().ok()?;
        let month: u32 = caps[2].parse().ok()?;
        return NaiveDate::from_ymd_opt(year, month, 1);
    }

    None
}

// ── Step 6: 合計・稼働日数 ──

fn detect_totals(
    rows: &[Vec<Data>],
    daily_data: &[DailyEntry],
) -> (Decimal, i32) {
    let total_from_data: f64 = daily_data.iter().map(|d| d.hours).sum();
    let days_from_data = daily_data.iter().filter(|d| d.hours > 0.0).count() as i32;

    // E41（時間合計）、E42（稼働日数）もチェック
    let e41 = rows.get(40).and_then(|r| r.get(4)).and_then(cell_to_f64);
    let e42 = rows.get(41).and_then(|r| r.get(4)).and_then(cell_to_f64);

    let total = if let Some(v) = e41 {
        if v > 0.0 { Decimal::from_f64_retain(v).unwrap_or(Decimal::ZERO) }
        else { Decimal::from_f64_retain(total_from_data).unwrap_or(Decimal::ZERO) }
    } else {
        Decimal::from_f64_retain(total_from_data).unwrap_or(Decimal::ZERO)
    };

    let days = if let Some(v) = e42 {
        if v > 0.0 { v as i32 } else { days_from_data }
    } else {
        days_from_data
    };

    (total, days)
}

// ── Step 7: 作業者名 ──

fn extract_worker_name_from_filename(filename: &str) -> String {
    // ファイル名から「（名前）」パターン
    let re = regex::Regex::new(r"[（(](.+?)[）)]").ok();
    if let Some(re) = re {
        if let Some(caps) = re.captures(filename) {
            let name = &caps[1];
            let skip_keywords = ["報告", "作業", "コピー", "copy"];
            if !skip_keywords.iter().any(|kw| name.contains(kw)) {
                return name.to_string();
            }
        }
    }
    String::new()
}

// ── Step 8: 稼働チェック ──

pub(crate) fn check_work_alerts(daily_data: &[DailyEntry]) -> Vec<WorkAlert> {
    let mut alerts = Vec::new();

    for entry in daily_data {
        if entry.hours <= 0.0 {
            continue;
        }

        let d = match NaiveDate::parse_from_str(&entry.date, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => continue,
        };

        // 1. 土日チェック
        let weekday = d.weekday().num_days_from_monday();
        if weekday >= 5 {
            let day_type = if weekday == 5 { "土曜" } else { "日曜" };
            alerts.push(WorkAlert {
                date: entry.date.clone(),
                alert_type: "weekend".to_string(),
                hours: entry.hours,
                day_name: entry.day_name.clone(),
                detail: format!("{}の稼働: {}h", day_type, entry.hours),
            });
        }

        // 1.5 実働時間（アプリの計算値）が、勤務表の記載と合わない日
        if let Some(stated) = entry.stated_hours {
            if (entry.hours - stated).abs() > 0.01 {
                alerts.push(WorkAlert {
                    date: entry.date.clone(),
                    alert_type: "hours_mismatch".to_string(),
                    hours: entry.hours,
                    day_name: entry.day_name.clone(),
                    detail: format!(
                        "開始・終了・休憩から計算した実働は{}h、勤務表の記載は{}h です。計算値で登録します。",
                        entry.hours, stated
                    ),
                });
            }
        }

        // 2. 15分単位チェック
        let remainder = (entry.hours % 0.25 * 10000.0).round() / 10000.0;
        if remainder.abs() > 0.001 {
            alerts.push(WorkAlert {
                date: entry.date.clone(),
                alert_type: "time_unit".to_string(),
                hours: entry.hours,
                day_name: entry.day_name.clone(),
                detail: format!("稼働時間 {}h が15分単位ではありません", entry.hours),
            });
        }
    }

    alerts
}

// ── ユーティリティ ──

fn cell_to_f64(cell: &Data) -> Option<f64> {
    match cell {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        Data::String(s) => s.parse::<f64>().ok(),
        Data::DateTime(dt) => Some(dt.as_f64()),
        _ => None,
    }
}

fn cell_to_date(cell: &Data) -> Option<NaiveDate> {
    match cell {
        Data::DateTime(dt) => {
            // ExcelDateTime → f64 → NaiveDate
            let serial = dt.as_f64() as i64;
            serial_to_date(serial)
        }
        Data::DateTimeIso(s) => {
            NaiveDate::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()
                .or_else(|| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        }
        Data::Float(f) => {
            let serial = *f as i64;
            serial_to_date(serial)
        }
        _ => None,
    }
}

/// `cell_to_date`の年月コンテキスト対応版。
///
/// 「稼動年月」ヘッダーが検出できている場合、日付列の値が1〜31の小さい整数であれば
/// Excelシリアル値としてではなく、その年月内の「日」として解釈する
/// （例: A列に日にちのみが入力され、年月は別セルで指定される勤務表フォーマット）。
/// それ以外（本物のExcel日付型・シリアル値、または年月ヘッダー未検出）は従来通り。
fn cell_to_date_with_ym(cell: &Data, year_month: Option<(i32, u32)>) -> Option<NaiveDate> {
    if let (Data::Float(f), Some((year, month))) = (cell, year_month) {
        if *f >= 1.0 && *f <= 31.0 && *f == f.floor() {
            if let Some(d) = NaiveDate::from_ymd_opt(year, month, *f as u32) {
                return Some(d);
            }
        }
    }
    cell_to_date(cell)
}

/// Excelシリアル日付 → NaiveDate
fn serial_to_date(serial: i64) -> Option<NaiveDate> {
    if serial < 1 || serial > 80000 { return None; }
    // Excelの1900年バグ（1900-02-29が存在する）を考慮
    let adjusted = if serial > 59 { serial - 2 } else { serial - 1 };
    NaiveDate::from_ymd_opt(1900, 1, 1)
        .and_then(|base| base.checked_add_signed(chrono::Duration::days(adjusted)))
}

/// Excelの時刻シリアル値（0.0〜1.0）または文字列を HH:MM 形式に変換
fn cell_to_time_str(cell: &Data) -> Option<String> {
    match cell {
        Data::Float(f) => serial_to_time(*f),
        Data::DateTime(dt) => {
            let f = dt.as_f64();
            // 時刻部分は小数部
            let frac = f - f.floor();
            serial_to_time(frac)
        }
        Data::String(s) => {
            let s = s.trim();
            if s.is_empty() { return None; }
            // "9:00" or "09:00" or "9:00:00" 形式
            if s.contains(':') {
                Some(s.split(':').take(2).collect::<Vec<_>>().join(":"))
            } else {
                None
            }
        }
        Data::Int(i) => {
            // 整数の場合、時間として解釈（例: 9 → "9:00"）
            if *i >= 0 && *i <= 24 {
                Some(format!("{}:00", i))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// 時刻シリアル値（0.0 = 0:00, 0.5 = 12:00, 1.0 = 24:00）→ "HH:MM"
/// 0.0（0:00）は未入力とみなしNoneを返す
fn serial_to_time(frac: f64) -> Option<String> {
    if frac < 0.01 || frac > 1.5 { return None; }
    let total_minutes = (frac * 24.0 * 60.0).round() as i32;
    let h = total_minutes / 60;
    let m = total_minutes % 60;
    Some(format!("{}:{:02}", h, m))
}

// ── Step 3.5: 開始・終了時刻列検出 ──

/// 時刻列情報（分割パターン or 単一列パターン）
#[derive(Debug, Default)]
struct TimeCols {
    /// 開始時刻: (時列, 分列) または (単一列, None)
    start: Option<(usize, Option<usize>)>,
    /// 終了時刻: (時列, 分列) または (単一列, None)
    end: Option<(usize, Option<usize>)>,
    /// 休憩（分）の列。ヘッダーの文字で見つけた場合だけ入る
    break_col: Option<usize>,
    /// 開始・終了・休憩の列を、ヘッダーの文字（「開始時間」「終了時間」「休憩時間」）で確実に見つけたか。
    /// true のときだけ、実働時間を「終了−開始−休憩」で計算する（列の推定で見つけた場合は、誤った時間を出さないよう計算しない）
    by_header: bool,
}

impl TimeCols {
    fn has_times(&self) -> bool {
        self.start.is_some()
    }

    /// 行から開始時刻を読み取る
    fn read_start(&self, row: &[Data]) -> String {
        self.read_time(row, &self.start)
    }

    /// 行から終了時刻を読み取る
    fn read_end(&self, row: &[Data]) -> String {
        self.read_time(row, &self.end)
    }

    /// 行から休憩（分）を読み取る。列が無い、または読めないときは None
    fn read_break(&self, row: &[Data]) -> Option<i32> {
        let col = self.break_col?;
        let v = row.get(col).and_then(cell_to_f64)?;
        if (0.0..=600.0).contains(&v) { Some(v.round() as i32) } else { None }
    }

    fn read_time(&self, row: &[Data], cols: &Option<(usize, Option<usize>)>) -> String {
        match cols {
            Some((hour_col, Some(min_col))) => {
                // 分割パターン: 時列 + 分列
                let h = row.get(*hour_col).and_then(cell_to_int).unwrap_or(-1);
                let m = row.get(*min_col).and_then(cell_to_int).unwrap_or(-1);
                if h >= 0 && h <= 23 && m >= 0 && m <= 59 {
                    format!("{}:{:02}", h, m)
                } else {
                    String::new()
                }
            }
            Some((col, None)) => {
                // 単一列パターン
                row.get(*col)
                    .and_then(cell_to_time_str)
                    .unwrap_or_default()
            }
            None => String::new(),
        }
    }
}

/// セルを整数として読み取る
fn cell_to_int(cell: &Data) -> Option<i32> {
    match cell {
        Data::Int(i) => Some(*i as i32),
        Data::Float(f) => Some(*f as i32),
        Data::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// 「時列 + ： + 分列」の分割パターンを検出する
/// パターン: [H列]整数(0-23) + [コロン列]"：" + [M列]整数(0-59)
fn detect_time_columns(
    rows: &[Vec<Data>],
    date_col: usize,
    hours_col: usize,
    data_rows: &[usize],
) -> TimeCols {
    // ヘッダーの文字で見つかる書式（開始・終了・休憩の列が明記されている）を、最優先にする
    if let Some(cols) = detect_time_columns_by_header(rows, data_rows) {
        return cols;
    }

    let max_col = rows.iter().map(|r| r.len()).max().unwrap_or(0).min(25);

    // コロンセパレータ列を探す（「：」または「:」）
    let mut colon_cols: Vec<usize> = Vec::new();
    for col_idx in (date_col + 1)..max_col {
        let mut colon_count = 0;
        for &row_idx in data_rows.iter().take(10) {
            if let Some(row) = rows.get(row_idx) {
                if let Some(Data::String(s)) = row.get(col_idx) {
                    let s = s.trim();
                    if s == "：" || s == ":" {
                        colon_count += 1;
                    }
                }
            }
        }
        // データ行の半分以上がコロンならセパレータ列
        if colon_count >= 3 {
            colon_cols.push(col_idx);
        }
    }

    // コロン列の前後に時・分の整数列があるかチェック
    let mut time_pairs: Vec<(usize, usize)> = Vec::new(); // (hour_col, min_col)
    for &colon_col in &colon_cols {
        if colon_col == 0 || colon_col + 1 >= max_col { continue; }
        let hour_col = colon_col - 1;
        let min_col = colon_col + 1;
        if hour_col == hours_col || min_col == hours_col { continue; }

        // 確認: hour_colに0-23の整数、min_colに0-59の整数があるか
        let mut valid = 0;
        for &row_idx in data_rows.iter().take(10) {
            if let Some(row) = rows.get(row_idx) {
                let h = row.get(hour_col).and_then(cell_to_int).unwrap_or(-1);
                let m = row.get(min_col).and_then(cell_to_int).unwrap_or(-1);
                if h >= 0 && h <= 23 && m >= 0 && m <= 59 {
                    valid += 1;
                }
            }
        }
        if valid >= 3 {
            time_pairs.push((hour_col, min_col));
        }
    }

    // 最初の2ペアが開始・終了
    if time_pairs.len() >= 2 {
        return TimeCols {
            start: Some((time_pairs[0].0, Some(time_pairs[0].1))),
            end: Some((time_pairs[1].0, Some(time_pairs[1].1))),
            ..Default::default()
        };
    }
    if time_pairs.len() == 1 {
        return TimeCols {
            start: Some((time_pairs[0].0, Some(time_pairs[0].1))),
            end: None,
            ..Default::default()
        };
    }

    // 分割パターンがない場合、単一列の時刻シリアル値も探す
    let mut serial_time_cols: Vec<(usize, i32)> = Vec::new();
    for col_idx in (date_col + 1)..max_col {
        if col_idx == hours_col { continue; }
        let mut count = 0i32;
        let mut values: Vec<f64> = Vec::new();
        for &row_idx in data_rows {
            if let Some(row) = rows.get(row_idx) {
                if let Some(cell) = row.get(col_idx) {
                    match cell {
                        Data::Float(f) if *f > 0.2 && *f < 0.96 => {
                            count += 1;
                            values.push(*f);
                        }
                        Data::DateTime(dt) => {
                            let frac = dt.as_f64() - dt.as_f64().floor();
                            if frac > 0.2 && frac < 0.96 {
                                count += 1;
                                values.push(frac);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        // 全値同一なら除外
        if !values.is_empty() {
            let first = values[0];
            if values.iter().all(|v| (v - first).abs() < 0.001) { continue; }
        }
        if count >= 5 && count as usize >= data_rows.len() / 3 {
            serial_time_cols.push((col_idx, count));
        }
    }
    serial_time_cols.sort_by_key(|c| c.0);

    TimeCols {
        start: serial_time_cols.first().map(|c| (c.0, None)),
        end: serial_time_cols.get(1).map(|c| (c.0, None)),
        ..Default::default()
    }
}

/// ヘッダー行の文字から、開始・終了・休憩の列を見つける（「開始時間」「終了時間」「休憩\n時間」など）。
///
/// 全日が同じ値の列（例: 開始が毎日 9:00）も、ヘッダーで見つかれば使う。
/// 開始・終了の両方が見つかり、データ行に時刻が入っているときだけ採用する。
fn detect_time_columns_by_header(rows: &[Vec<Data>], data_rows: &[usize]) -> Option<TimeCols> {
    let first_data = *data_rows.first()?;
    let (mut start, mut end, mut brk) = (None, None, None);
    for row in rows.iter().take(first_data) {
        for (col, cell) in row.iter().enumerate() {
            let Data::String(text) = cell else { continue };
            let t: String = text.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect();
            if t.is_empty() || t.chars().count() > 8 {
                continue; // 見出しは短い。長い文（注意書きなど）は除く
            }
            if start.is_none() && t.contains("開始") {
                start = Some(col);
            } else if end.is_none() && t.contains("終了") {
                end = Some(col);
            } else if brk.is_none() && t.contains("休憩") {
                brk = Some(col);
            }
        }
    }
    let (start, end) = (start?, end?);
    // データ行に、開始・終了とも時刻が入っていること（見出しだけの誤検出を避ける）
    let has_times = |col: usize| {
        data_rows
            .iter()
            .filter(|&&r| rows.get(r).and_then(|row| row.get(col)).and_then(cell_to_time_str).is_some())
            .count()
            >= 3
    };
    if !(has_times(start) && has_times(end)) {
        return None;
    }
    Some(TimeCols {
        start: Some((start, None)),
        end: Some((end, None)),
        break_col: brk,
        by_header: brk.is_some(),
    })
}

// ── テスト ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_worker_name_from_filename() {
        assert_eq!(
            extract_worker_name_from_filename("稼働報告書（田中太郎）.xlsx"),
            "田中太郎"
        );
        assert_eq!(
            extract_worker_name_from_filename("report(Yamada).xlsx"),
            "Yamada"
        );
        // 除外キーワード
        assert_eq!(
            extract_worker_name_from_filename("稼働報告書（報告書コピー）.xlsx"),
            ""
        );
    }

    #[test]
    fn test_serial_to_date() {
        // 2024-01-15 = serial 45306
        let d = serial_to_date(45306).unwrap();
        assert_eq!(d, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
    }

    #[test]
    fn test_check_alerts_weekend() {
        let data = vec![DailyEntry {
            date: "2024-01-13".to_string(), // 土曜
            day_name: "土".to_string(),
            hours: 8.0,
            start: String::new(),
            end: String::new(),
            ..Default::default()
        }];
        let alerts = check_work_alerts(&data);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "weekend");
    }

    /// 「稼動年月」見出しのある勤務表フォーマット（A列=日にちのみ、B列=曜日、
    /// 年月は稼動年月ラベルセルに別掲、氏名もI3セルに別掲）の実物サンプルで
    /// 正しく解析できることを確認する回帰テスト。
    /// このフォーマットで過去に発生していた不具合:
    /// - score_sheetがB列固定で日付列を判定しており、A列に日付があるこのフォーマットで
    ///   全シートがスコア0になり「シートが見つかりません」エラーになっていた
    /// - is_date_valueの`> 1.0`が月初日(値=1)を除外していた
    /// - 日にちのみの値(1〜31)をExcelシリアル値として解釈し1900年扱いになっていた
    #[test]
    fn parses_year_month_header_timesheet_format() {
        let bytes = std::fs::read("docs/templates/勤務表_yyyy年mm月_氏名フルネーム.xlsx").unwrap();
        let result = auto_detect_and_parse(&bytes, "勤務表_2026年7月_山田太郎.xlsx");

        assert!(result.error.is_none(), "解析エラー: {:?}", result.error);
        assert_eq!(result.sheet_name, "出勤簿");
        assert_eq!(result.target_month, NaiveDate::from_ymd_opt(2026, 7, 1));
        assert_eq!(result.daily_data.len(), 31);
        assert_eq!(result.daily_data[0].date, "2026-07-01");
        assert_eq!(result.daily_data[0].hours, 8.0);
        assert_eq!(result.work_days, 1);
    }

    #[test]
    fn test_check_alerts_time_unit() {
        let data = vec![DailyEntry {
            date: "2024-01-15".to_string(), // 月曜
            day_name: "月".to_string(),
            hours: 7.33, // 15分単位でない
            start: String::new(),
            end: String::new(),
            ..Default::default()
        }];
        let alerts = check_work_alerts(&data);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert_type, "time_unit");
    }

    /// 拡張子欠落／誤拡張子でも `%PDF` マジックバイトで PDF の経路へ振り分けること（Excel(ZIP) 経路に落とさない）。
    /// PDF の勤務表の様式は取引先ごとのカスタマイズが読むため、ここでは「PDF として扱われた」ことだけを確かめる。
    /// 様式どおりに読めることは、カスタマイズ側（src/custom/ の勤務表 PDF の様式）のテストで確かめる。
    #[test]
    fn routes_pdf_by_magic_bytes_to_pdf_path() {
        let bytes = b"%PDF-1.4 broken";
        for name in ["unknown.xlsx", "upload.bin", ""] {
            let result = auto_detect_and_parse(bytes, name);
            let err = result.error.unwrap_or_default();
            assert!(err.contains("PDF"), "filename={name:?} が PDF の経路に振り分けられていない: {err}");
        }
    }

    fn f(v: f64) -> Data { Data::Float(v) }
    fn t(v: &str) -> Data { Data::String(v.to_string()) }

    /// 開始・終了・休憩の見出しがある書式（開始が毎日同じ 9:00）でも、列をヘッダーで見つけること
    #[test]
    fn detects_time_columns_by_header_even_when_start_is_constant() {
        // 列: 0=日付 1=曜日 2=区分 3=開始 4=終了 5=休憩
        let mut rows = vec![
            vec![t("勤務表")],
            vec![t("日付"), t("曜日"), t("区分"), t("開始時間"), t("終了時間"), t("休憩\n時間")],
        ];
        let data_start = rows.len();
        for day in 1..=5 {
            // 9:00 = 0.375、終了は日によって違う（0.75=18:00, 0.8333=20:00）
            let end = if day % 2 == 0 { 0.75 } else { 0.8333333 };
            rows.push(vec![f(day as f64), t("月"), Data::Empty, f(0.375), f(end), f(60.0)]);
        }
        let data_rows: Vec<usize> = (data_start..rows.len()).collect();
        let cols = detect_time_columns_by_header(&rows, &data_rows).expect("ヘッダーで見つかるはず");
        assert_eq!(cols.start, Some((3, None)));
        assert_eq!(cols.end, Some((4, None)));
        assert_eq!(cols.break_col, Some(5));
        assert!(cols.by_header);
        let row = &rows[data_start];
        assert_eq!(cols.read_start(row), "9:00");
        assert_eq!(cols.read_break(row), Some(60));
    }

    #[test]
    fn header_detection_needs_times_in_the_data_rows() {
        // 「開始」「終了」の文字があっても、データ行に時刻が無ければ採用しない（注意書きなどの誤検出を避ける）
        let rows = vec![
            vec![t("開始時間"), t("終了時間")],
            vec![f(1.0), f(2.0)], vec![f(2.0), f(3.0)], vec![f(3.0), f(4.0)],
        ];
        assert!(detect_time_columns_by_header(&rows, &[1, 2, 3]).is_none());
    }

    #[test]
    fn computes_work_hours_from_start_end_and_break() {
        assert_eq!(compute_work_hours("9:00", "19:30", 60), Some(9.5));
        assert_eq!(compute_work_hours("09:00", "09:30", 60), None);
        assert_eq!(compute_work_hours("18:00", "09:00", 60), None);
    }

    #[test]
    fn finds_project_name_next_to_its_label() {
        let rows = vec![
            vec![t("勤務表")],
            vec![t("会社名"), Data::Empty, Data::Empty, t("サンプル商事株式会社")],
            vec![t("案件名"), Data::Empty, Data::Empty, t("PM支援業務")],
        ];
        assert_eq!(detect_project_name(&rows), "PM支援業務");
        assert_eq!(detect_project_name(&[vec![t("勤務表")]]), "");
    }
}
