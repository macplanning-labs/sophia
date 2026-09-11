/// presentation/handlers/timesheet_self_report.rs — 案件に紐づかない社員向け「月次稼働報告（自己申告）」
///
/// 稼働報告（t_monthly_timesheet）は本来 client_contract_id（案件）に紐づくが、
/// 案件を持たない社員（バックオフィス等）はそれでは稼働報告を登録できない。
/// ここでは employee_id 直接紐付けの自己申告経路を提供する（2026-07-30追加）。
///
/// ## エンドポイント
/// - GET  /api/v1/timesheets/self-report               — 当月分の既存提出内容を取得（再編集用）
/// - POST /api/v1/timesheets/self-report                — 画面入力（日次内訳込み）を提出
/// - POST /api/v1/timesheets/self-report/sheets/prepare — Googleスプレッドシート・テンプレートを複製・共有
/// - POST /api/v1/timesheets/self-report/sheets/submit  — 複製したスプレッドシートの内容を取り込んで提出

use axum::{
    extract::{Extension, Query, State},
    response::IntoResponse,
};
use chrono::{Datelike, NaiveDate, Weekday};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::infrastructure::repositories::{payroll_repo, timesheet_repo};
use crate::infrastructure::sheets_service;
use crate::presentation::middleware::role::AuthUser;

// ── 対象月パース ──

/// "2026-07" または "2026-07-01" を月初日に正規化する
fn parse_target_month(raw: &str) -> Option<NaiveDate> {
    let s = raw.trim();
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return NaiveDate::from_ymd_opt(d.year(), d.month(), 1);
    }
    NaiveDate::parse_from_str(&format!("{}-01", s), "%Y-%m-%d").ok()
}

fn unauthorized() -> axum::response::Response {
    (
        axum::http::StatusCode::FORBIDDEN,
        axum::Json(serde_json::json!({"success": false, "error": "社員アカウントでログインしてください"})),
    )
        .into_response()
}

// ── 日次内訳 ──

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct DailyEntry {
    pub day: u32,
    #[serde(default)]
    pub start: String,
    #[serde(default)]
    pub end: String,
    #[serde(default)]
    pub break_minutes: i32,
    #[serde(default)]
    pub off: bool,
}

fn parse_hhmm(s: &str) -> Option<i32> {
    let (h, m) = s.split_once(':')?;
    let h: i32 = h.trim().parse().ok()?;
    let m: i32 = m.trim().parse().ok()?;
    Some(h * 60 + m)
}

fn worked_hours(entry: &DailyEntry) -> Decimal {
    if entry.off {
        return Decimal::ZERO;
    }
    let (Some(s), Some(e)) = (parse_hhmm(&entry.start), parse_hhmm(&entry.end)) else {
        return Decimal::ZERO;
    };
    let mins = (e - s) - entry.break_minutes;
    if mins <= 0 {
        return Decimal::ZERO;
    }
    Decimal::new(mins as i64, 0) / Decimal::new(60, 0)
}

/// 日次内訳から給与計算に使う5項目（総稼働時間・稼働日数・残業時間・休日出勤時間）を集計する。
/// 深夜時間はここでは算出しない（時刻またぎの計算が複雑なため、フォーム側の手入力値をそのまま使う）。
/// スプレッドシート/画面側の合計値をそのまま信用せず、日次内訳からサーバー側で再計算する。
fn summarize_daily_entries(target_month: NaiveDate, entries: &[DailyEntry]) -> (Decimal, i32, Decimal, Decimal) {
    let mut total = Decimal::ZERO;
    let mut days = 0i32;
    let mut overtime = Decimal::ZERO;
    let mut holiday = Decimal::ZERO;
    let eight = Decimal::new(8, 0);

    for entry in entries {
        let worked = worked_hours(entry);
        if worked <= Decimal::ZERO {
            continue;
        }
        days += 1;
        total += worked;
        if worked > eight {
            overtime += worked - eight;
        }
        if let Some(date) = NaiveDate::from_ymd_opt(target_month.year(), target_month.month(), entry.day) {
            if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
                holiday += worked;
            }
        }
    }

    (total, days, overtime, holiday)
}

// ── GET: 当月分の既存提出内容を取得 ──

#[derive(Debug, serde::Deserialize)]
pub struct SelfReportQuery {
    pub month: String,
}

pub async fn api_get(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Query(q): Query<SelfReportQuery>,
) -> impl IntoResponse {
    let Some(employee_id) = auth_user.employee_id() else {
        return unauthorized();
    };
    let Some(target_month) = parse_target_month(&q.month) else {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "対象月の形式が不正です（YYYY-MM）"}))).into_response();
    };

    match timesheet_repo::find_self_report(&pool, employee_id, target_month).await {
        Ok(sheet) => axum::Json(serde_json::json!({ "sheet": sheet })).into_response(),
        Err(e) => {
            tracing::error!("self-report取得エラー: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"success": false, "error": "取得に失敗しました"}))).into_response()
        }
    }
}

// ── POST: 画面入力（日次内訳込み）を提出 ──

#[derive(Debug, serde::Deserialize)]
pub struct SelfReportSubmitForm {
    pub target_month: String,
    pub daily_data: Vec<DailyEntry>,
    #[serde(default)]
    pub night_hours: Decimal,
}

pub async fn api_submit(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    axum::Json(form): axum::Json<SelfReportSubmitForm>,
) -> impl IntoResponse {
    let Some(employee_id) = auth_user.employee_id() else {
        return unauthorized();
    };
    let Some(target_month) = parse_target_month(&form.target_month) else {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "対象月の形式が不正です（YYYY-MM）"}))).into_response();
    };

    if let Ok(Some(existing)) = timesheet_repo::find_self_report(&pool, employee_id, target_month).await {
        if existing.status == "APPROVED" {
            return (axum::http::StatusCode::CONFLICT,
                axum::Json(serde_json::json!({"success": false, "error": "承認済みの月は再編集できません"}))).into_response();
        }
    }

    let (total_hours, work_days, overtime_hours, holiday_hours) = summarize_daily_entries(target_month, &form.daily_data);
    let daily_json = match serde_json::to_value(&form.daily_data) {
        Ok(v) => v,
        Err(_) => serde_json::Value::Null,
    };

    match timesheet_repo::upsert_self_report(
        &pool, employee_id, target_month, &daily_json,
        total_hours, work_days, overtime_hours, form.night_hours, holiday_hours,
        auth_user.user.id,
    ).await {
        Ok(id) => axum::Json(serde_json::json!({
            "success": true,
            "id": id,
            "message": "提出しました。管理者の承認をもって給与計算に反映されます。",
            "total_hours": total_hours,
            "work_days": work_days,
            "overtime_hours": overtime_hours,
            "holiday_hours": holiday_hours,
        })).into_response(),
        Err(e) => {
            tracing::error!("self-report提出エラー: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"success": false, "error": "提出に失敗しました"}))).into_response()
        }
    }
}

// ── POST: Googleスプレッドシート・テンプレートを複製・共有 ──

#[derive(Debug, serde::Deserialize)]
pub struct SheetsPrepareForm {
    pub target_month: String,
}

pub async fn api_sheets_prepare(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    axum::Json(form): axum::Json<SheetsPrepareForm>,
) -> impl IntoResponse {
    let Some(employee_id) = auth_user.employee_id() else {
        return unauthorized();
    };
    let Some(target_month) = parse_target_month(&form.target_month) else {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "対象月の形式が不正です（YYYY-MM）"}))).into_response();
    };

    if let Ok(Some(existing)) = timesheet_repo::find_self_report(&pool, employee_id, target_month).await {
        if existing.status == "APPROVED" {
            return (axum::http::StatusCode::CONFLICT,
                axum::Json(serde_json::json!({"success": false, "error": "承認済みの月は再編集できません"}))).into_response();
        }
    }

    let employee_name = payroll_repo::find_employee_full_name(&pool, employee_id).await
        .unwrap_or_else(|_| auth_user.user.username.clone());
    let sheet_name = format!("稼働報告_{}_{}", target_month.format("%Y年%m月"), employee_name);

    match sheets_service::copy_template_and_share(&auth_user.user.email, &sheet_name).await {
        Ok(Some((file_id, url))) => {
            if let Err(e) = timesheet_repo::upsert_self_report_sheet_link(&pool, employee_id, target_month, &file_id).await {
                tracing::error!("sheet_file_id保存エラー: {:?}", e);
                return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({"success": false, "error": "スプレッドシート情報の保存に失敗しました"}))).into_response();
            }
            axum::Json(serde_json::json!({ "success": true, "url": url })).into_response()
        }
        Ok(None) => (axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({"success": false, "error": "Googleスプレッドシート連携が設定されていません"}))).into_response(),
        Err(e) => {
            tracing::error!("スプレッドシート複製エラー: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"success": false, "error": "スプレッドシートの準備に失敗しました"}))).into_response()
        }
    }
}

// ── POST: 複製したスプレッドシートの内容を取り込んで提出 ──

#[derive(Debug, serde::Deserialize)]
pub struct SheetsSubmitForm {
    pub target_month: String,
}

/// テンプレート固定レイアウト（build_timesheet_template.py と同じ配置）:
/// 9行目〜39行目、A列=日、C列=開始、D列=終了、E列=休憩(分)、F列=休フラグ('○'なら休み)
const SHEET_RANGE: &str = "稼働報告!A9:F39";

fn cell_str(row: &[serde_json::Value], idx: usize) -> String {
    row.get(idx).and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or_default()
}

pub async fn api_sheets_submit(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    axum::Json(form): axum::Json<SheetsSubmitForm>,
) -> impl IntoResponse {
    let Some(employee_id) = auth_user.employee_id() else {
        return unauthorized();
    };
    let Some(target_month) = parse_target_month(&form.target_month) else {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "対象月の形式が不正です（YYYY-MM）"}))).into_response();
    };

    let existing = timesheet_repo::find_self_report(&pool, employee_id, target_month).await.ok().flatten();
    let Some(existing) = existing else {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "先にスプレッドシートを開いて作成してください"}))).into_response();
    };
    if existing.status == "APPROVED" {
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"success": false, "error": "承認済みの月は再編集できません"}))).into_response();
    }
    if existing.sheet_file_id.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "先にスプレッドシートを開いて作成してください"}))).into_response();
    }

    let rows = match sheets_service::read_values(&existing.sheet_file_id, SHEET_RANGE).await {
        Ok(Some(rows)) => rows,
        Ok(None) => return (axum::http::StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({"success": false, "error": "Googleスプレッドシート連携が設定されていません"}))).into_response(),
        Err(e) => {
            tracing::error!("スプレッドシート読み取りエラー: {:?}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"success": false, "error": "スプレッドシートの読み取りに失敗しました"}))).into_response();
        }
    };

    let daily_data: Vec<DailyEntry> = rows.iter().enumerate().map(|(i, row)| {
        let day = (i as u32) + 1;
        let off = cell_str(row, 5) == "○"; // F列
        DailyEntry {
            day,
            start: cell_str(row, 2), // C列
            end: cell_str(row, 3),   // D列
            break_minutes: cell_str(row, 4).parse().unwrap_or(0), // E列
            off,
        }
    }).collect();

    let (total_hours, work_days, overtime_hours, holiday_hours) = summarize_daily_entries(target_month, &daily_data);
    let daily_json = serde_json::to_value(&daily_data).unwrap_or(serde_json::Value::Null);
    let night_hours = existing.night_hours; // 深夜時間は画面入力タブの手入力値を維持（Sheets取り込みでは変更しない）

    match timesheet_repo::upsert_self_report(
        &pool, employee_id, target_month, &daily_json,
        total_hours, work_days, overtime_hours, night_hours, holiday_hours,
        auth_user.user.id,
    ).await {
        Ok(id) => axum::Json(serde_json::json!({
            "success": true,
            "id": id,
            "message": "スプレッドシートの内容を取り込みました。管理者の承認をもって給与計算に反映されます。",
            "total_hours": total_hours,
            "work_days": work_days,
            "overtime_hours": overtime_hours,
            "holiday_hours": holiday_hours,
        })).into_response(),
        Err(e) => {
            tracing::error!("self-report取り込みエラー: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"success": false, "error": "取り込みに失敗しました"}))).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(day: u32, start: &str, end: &str, brk: i32, off: bool) -> DailyEntry {
        DailyEntry { day, start: start.into(), end: end.into(), break_minutes: brk, off }
    }

    #[test]
    fn worked_hours_computes_standard_day() {
        let e = entry(1, "09:00", "18:00", 60, false);
        assert_eq!(worked_hours(&e), Decimal::new(8, 0));
    }

    #[test]
    fn worked_hours_is_zero_when_off() {
        let e = entry(1, "09:00", "18:00", 60, true);
        assert_eq!(worked_hours(&e), Decimal::ZERO);
    }

    #[test]
    fn worked_hours_is_zero_for_invalid_times() {
        let e = entry(1, "", "18:00", 60, false);
        assert_eq!(worked_hours(&e), Decimal::ZERO);
    }

    #[test]
    fn summarize_counts_days_total_and_overtime() {
        // 2026-07-01は水曜。平日3日分、うち1日は10時間勤務（残業2時間）
        let month = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let entries = vec![
            entry(1, "09:00", "18:00", 60, false), // 8h
            entry(2, "09:00", "20:00", 60, false), // 10h → 残業2h
            entry(3, "09:00", "18:00", 60, false), // 8h
        ];
        let (total, days, ot, holiday) = summarize_daily_entries(month, &entries);
        assert_eq!(total, Decimal::new(26, 0));
        assert_eq!(days, 3);
        assert_eq!(ot, Decimal::new(2, 0));
        assert_eq!(holiday, Decimal::ZERO);
    }

    #[test]
    fn summarize_counts_weekend_work_as_holiday_hours() {
        // 2026-07-04は土曜
        let month = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let entries = vec![entry(4, "09:00", "13:00", 0, false)]; // 4h 休日出勤
        let (total, days, ot, holiday) = summarize_daily_entries(month, &entries);
        assert_eq!(total, Decimal::new(4, 0));
        assert_eq!(days, 1);
        assert_eq!(ot, Decimal::ZERO);
        assert_eq!(holiday, Decimal::new(4, 0));
    }
}
