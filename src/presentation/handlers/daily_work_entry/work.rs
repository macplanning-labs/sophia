use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{Datelike, NaiveDate};
use sqlx::PgPool;
use crate::infrastructure::repositories::{order_repo, timesheet_repo};
use crate::presentation::middleware::role::AuthUser;
use super::{check_permission, calc_actual_minutes, parse_time, WorkEntryQuery, SaveWorkEntriesRequest};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GET /api/portal/work-entries
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 指定エンジニア・月の日次稼働一覧
pub async fn list_work_entries(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<WorkEntryQuery>,
) -> impl IntoResponse {
    // 権限チェック
    if !check_permission(&pool, &auth_user, params.engineer_id).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "このエンジニアへのアクセス権がありません"}))).into_response();
    }

    // 月の範囲
    let start_date = NaiveDate::from_ymd_opt(params.year, params.month as u32, 1)
        .unwrap_or(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let end_date = if params.month == 12 {
        NaiveDate::from_ymd_opt(params.year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(params.year, (params.month + 1) as u32, 1).unwrap()
    };

    let entries = timesheet_repo::list_work_entries(&pool, params.engineer_id, start_date, end_date)
        .await
        .unwrap_or_default();

    // ロック状態チェック（t_monthly_timesheetのlocked_at）
    let locked = timesheet_repo::is_timesheet_locked(&pool, params.engineer_id, start_date)
        .await
        .unwrap_or(false);

    Json(serde_json::json!({
        "entries": entries,
        "locked": locked,
        "year": params.year,
        "month": params.month,
    })).into_response()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// POST /api/portal/work-entries
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 日次稼働の一括保存（UPSERT）
pub async fn save_work_entries(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Json(body): Json<SaveWorkEntriesRequest>,
) -> impl IntoResponse {
    // 権限チェック
    if !check_permission(&pool, &auth_user, body.engineer_id).await {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({"ok": false, "error": "このエンジニアへのアクセス権がありません"}))).into_response();
    }

    // ロック状態チェック（保存対象月のいずれかがロック済みなら拒否）
    for entry in &body.entries {
        let month_start = NaiveDate::from_ymd_opt(entry.work_date.year(), entry.work_date.month(), 1);
        if let Some(ms) = month_start {
            let is_locked = timesheet_repo::is_timesheet_locked(&pool, body.engineer_id, ms)
                .await
                .unwrap_or(false);

            if is_locked {
                return (StatusCode::CONFLICT, Json(serde_json::json!({
                    "ok": false,
                    "error": format!("{}年{}月のデータは確定済みのため変更できません", entry.work_date.year(), entry.work_date.month()),
                }))).into_response();
            }
        }
    }

    // UPSERT 実行
    let mut saved = 0i32;
    for entry in &body.entries {
        let start = entry.start_time.as_deref().and_then(parse_time);
        let end = entry.end_time.as_deref().and_then(parse_time);
        let break_min = entry.break_minutes.unwrap_or(60);
        let is_holiday = entry.is_holiday.unwrap_or(false);
        let actual = calc_actual_minutes(start, end, break_min, is_holiday);
        let desc = entry.work_description.as_deref().unwrap_or("");

        let result = timesheet_repo::upsert_work_entry(
            &pool, body.engineer_id, entry.work_date, start, end, break_min, actual, desc, is_holiday,
        ).await;

        match result {
            Ok(_) => saved += 1,
            Err(e) => {
                tracing::error!("日次稼働保存エラー: date={}, error={}", entry.work_date, e);
                return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                    "ok": false,
                    "error": format!("{}の保存に失敗しました: {}", entry.work_date, e)
                }))).into_response();
            }
        }
    }

    (StatusCode::OK, Json(serde_json::json!({
        "ok": true,
        "saved": saved,
        "message": format!("{}件の稼働データを保存しました", saved)
    }))).into_response()
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GET /api/portal/work-entries/summary
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 月次集計（合計時間、稼働日数、未入力日リスト）
pub async fn work_entries_summary(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Query(params): Query<WorkEntryQuery>,
) -> impl IntoResponse {
    // エンジニア本人の場合: 自分のデータのみアクセス可能
    if let Some(own_id) = auth_user.engineer_id() {
        if own_id != params.engineer_id {
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "このエンジニアへのアクセス権がありません"}))).into_response();
        }
        // engineer_id が一致 → 集計処理へ進む
    } else {
        // パートナー担当者の場合: パートナーIDチェック
        let partner_id = match auth_user.partner_id() {
            Some(id) => id.to_string(),
            None => return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "パートナーログインが必要です"}))).into_response(),
        };

        // エンジニアが自社所属か確認
        let belongs = order_repo::engineer_belongs_to_partner(&pool, params.engineer_id, &partner_id)
            .await
            .unwrap_or(false);

        if !belongs {
            return (StatusCode::FORBIDDEN, Json(serde_json::json!({"error": "このエンジニアへのアクセス権がありません"}))).into_response();
        }
    }

    // 月の範囲
    let start_date = NaiveDate::from_ymd_opt(params.year, params.month as u32, 1)
        .unwrap_or(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
    let end_date = if params.month == 12 {
        NaiveDate::from_ymd_opt(params.year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(params.year, (params.month + 1) as u32, 1).unwrap()
    };

    // 集計クエリ
    let (work_days, total_minutes) = timesheet_repo::sum_work_minutes(&pool, params.engineer_id, start_date, end_date)
        .await
        .unwrap_or((0, 0));
    let total_hours = total_minutes as f64 / 60.0;

    // 入力済み日付リスト
    let entered_dates = timesheet_repo::list_entered_dates(&pool, params.engineer_id, start_date, end_date)
        .await
        .unwrap_or_default();

    // 平日（月〜金）の未入力日を計算
    let today = chrono::Local::now().date_naive();
    let check_end = if end_date <= today { end_date } else { today + chrono::Duration::days(1) };
    let mut missing_dates: Vec<NaiveDate> = Vec::new();
    let mut d = start_date;
    while d < check_end {
        let weekday = d.weekday();
        if weekday != chrono::Weekday::Sat && weekday != chrono::Weekday::Sun {
            if !entered_dates.contains(&d) {
                missing_dates.push(d);
            }
        }
        d += chrono::Duration::days(1);
    }

    Json(serde_json::json!({
        "year": params.year,
        "month": params.month,
        "work_days": work_days,
        "total_minutes": total_minutes,
        "total_hours": format!("{:.1}", total_hours),
        "total_display": format!("{}時間{}分", total_minutes / 60, total_minutes % 60),
        "missing_dates": missing_dates,
        "missing_count": missing_dates.len(),
    })).into_response()
}
