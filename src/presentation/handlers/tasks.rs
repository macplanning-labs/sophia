/// presentation/handlers/tasks.rs — タスク管理 CRUD + 月次一括生成
///
/// Phase 4-C: タスクダッシュボード。
///
/// ## エンドポイント
/// - GET  /tasks                    — タスクダッシュボード（完了率・期限超過表示）
/// - POST /tasks/generate           — 月次タスク一括生成（10種別 × エンジニア数）
/// - POST /tasks/{id}/complete      — タスク完了
/// - POST /tasks/{id}/skip          — タスクスキップ

use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::domain::models::task::{MonthlyTask, TaskType};
use crate::domain::services::business_day::{calculate_deadline, resolve_report_deadline_settings};
use crate::infrastructure::repositories::task_repo;

/// 月の末日を算出
fn end_of_month(date: chrono::NaiveDate) -> chrono::NaiveDate {
    use chrono::Datelike;
    let (y, m) = if date.month() == 12 {
        (date.year() + 1, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    chrono::NaiveDate::from_ymd_opt(y, m, 1)
        .and_then(|d| d.pred_opt())
        .unwrap_or(date)
}

// ── テンプレート ──


/// タスクサマリー
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TaskSummary {
    pub total: i64,
    pub done: i64,
    pub overdue: i64,
    pub pending: i64,
    pub completion_rate: i64,
}

// ── フィルタ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct TaskFilter {
    pub month: Option<String>,
    pub status: Option<String>,
}

// ── ハンドラ ──

/// POST /tasks/generate — 月次タスク一括生成
pub async fn generate(
    State(pool): State<PgPool>,
    Form(form): Form<GenerateForm>,
) -> impl IntoResponse {
    let work_month = match chrono::NaiveDate::parse_from_str(&form.work_month, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Redirect::to("/tasks"),
    };

    use chrono::Datelike;

    // 全エンジニア（有効契約あり）を取得
    let engineers = task_repo::list_active_engineers_for_task_generation(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("tasks: fetch_all failed: {:?}", e); vec![] });

    let all_types = TaskType::all();
    let mut created = 0i64;

    for target in &engineers {
        let eng_id = target.engineer_id;
        let proj_id = &target.project_id;
        let eng_name = &target.engineer_name;

        for task_type in &all_types {
            let type_str = task_type.as_str();
            // 既存チェック
            let exists = task_repo::task_exists(&pool, work_month, type_str, eng_id, proj_id)
                .await
                .unwrap_or(true);

            if exists {
                continue;
            }

            // 期限（種別ごとにデフォルト設定）
            let deadline = match task_type {
                TaskType::OrderCreate | TaskType::ReceivedOrder => {
                    work_month // 月初
                }
                TaskType::ReportUpload => {
                    // 案件単位の提出期限設定（未設定なら契約のreport_deadline_days_beforeにフォールバック）
                    let settings = resolve_report_deadline_settings(
                        target.report_deadline_type.as_deref().unwrap_or("RELATIVE"),
                        target.report_deadline_value,
                        target.report_deadline_holiday_rule.as_deref(),
                        target.contract_deadline_days_before.unwrap_or(5),
                    );
                    calculate_deadline(work_month.year(), work_month.month(), &settings)
                        .unwrap_or_else(|e| {
                            tracing::warn!("稼働報告提出期限の計算に失敗、当月末にフォールバック: {:?}", e);
                            end_of_month(work_month)
                        })
                }
                TaskType::InvoiceCreate | TaskType::InvoiceSend => {
                    // 翌月10日
                    work_month + chrono::Months::new(1) + chrono::Duration::days(9)
                }
                _ => {
                    // 翌月15日
                    work_month + chrono::Months::new(1) + chrono::Duration::days(14)
                }
            };

            if let Err(e) = task_repo::insert_task(&pool, proj_id, eng_id, work_month, type_str, eng_name, deadline).await {
                tracing::error!("DB error: {:?}", e);
            }

            created += 1;
        }
    }

    tracing::info!("月次タスク一括生成: {}件（{}分）", created, work_month);
    Redirect::to("/tasks")
}

/// POST /tasks/{id}/complete — タスク完了
pub async fn complete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(e) = task_repo::complete(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to("/tasks")
}

/// POST /tasks/{id}/skip — タスクスキップ
pub async fn skip(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(e) = task_repo::skip(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to("/tasks")
}

// ── フォーム ──

#[derive(Debug, serde::Deserialize)]
pub struct GenerateForm {
    pub work_month: String,
}

// SPA用 JSON API
#[derive(serde::Serialize)]
pub struct TaskApiResponse {
    pub tasks: Vec<MonthlyTask>,
    pub summary: TaskSummary,
}

pub async fn api_index(
    State(pool): State<PgPool>,
    Query(filter): Query<TaskFilter>,
) -> axum::Json<TaskApiResponse> {
    let month = filter.month.unwrap_or_default();
    let status_filter = filter.status.unwrap_or_default();

    let tasks = task_repo::list_recent(&pool, 200)
        .await
        .unwrap_or_else(|e| { tracing::warn!("tasks api: {:?}", e); vec![] });

    let today = chrono::Utc::now().date_naive();
    let filtered: Vec<MonthlyTask> = tasks.into_iter().filter(|t| {
        if !month.is_empty() && t.work_month.format("%Y-%m").to_string() != month { return false; }
        if !status_filter.is_empty() && t.status != status_filter { return false; }
        true
    }).collect();

    let done = filtered.iter().filter(|t| t.status == "DONE").count() as i64;
    let overdue = filtered.iter().filter(|t| t.status != "DONE" && t.status != "SKIPPED" && t.deadline < today).count() as i64;
    let total = filtered.len() as i64;
    let summary = TaskSummary {
        total,
        done,
        overdue,
        pending: total - done,
        completion_rate: if total > 0 { done * 100 / total } else { 0 },
    };

    axum::Json(TaskApiResponse { tasks: filtered, summary })
}
