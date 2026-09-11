/// presentation/handlers/payroll.rs — 給与管理 CRUD + 一括計算
///
/// Phase 4-E: 給与計算機能。
///
/// ## エンドポイント
/// - GET  /payroll                  — 月次給与一覧（年月フィルタ）
/// - POST /payroll/calculate        — 月次一括計算
/// - GET  /payroll/{id}             — 給与明細表示
/// - POST /payroll/{id}/confirm     — 給与確認
/// - POST /payroll/{id}/paid        — 振込済

use axum::{
    extract::{Extension, Path, Query, State},
    response::{IntoResponse, Redirect},
    Form,
};
use chrono::Datelike;
use sqlx::PgPool;

use crate::domain::models::payroll::{PayrollCalcForm, PayrollWithEmployee};
use crate::domain::services::payroll_calculator::PayrollCalculator;
use crate::infrastructure::repositories::{paid_leave_repo, payroll_repo};
use crate::presentation::middleware::role::AuthUser;

/// 給与確定前に、対象社員・対象月の有給休暇（付与・失効・当月消化）を処理し、
/// 結果を t_payroll にスナップショットする
async fn apply_paid_leave_snapshot(pool: &PgPool, payroll_id: i64) -> anyhow::Result<()> {
    let Some(pay) = payroll_repo::find_payroll(pool, payroll_id).await? else {
        return Ok(());
    };
    let Some(employee) = payroll_repo::find_employee(pool, pay.employee_id).await? else {
        return Ok(());
    };
    let (used, balance) = paid_leave_repo::process_month_end(
        pool,
        employee.id,
        employee.hire_date,
        employee.weekly_prescribed_days,
        pay.year_month,
    )
    .await?;
    paid_leave_repo::snapshot_payroll(pool, payroll_id, used, balance).await?;
    Ok(())
}

// ── テンプレート ──



/// 給与サマリー
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PayrollSummary {
    pub total_count: i64,
    pub total_gross: i64,
    pub total_deduction: i64,
    pub total_net: i64,
}

// ── フィルタ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct PayrollFilter {
    pub month: Option<String>,
}

// ── ハンドラ ──

/// POST /payroll/calculate — 月次一括計算
pub async fn calculate(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Form(form): Form<PayrollCalcForm>,
) -> impl IntoResponse {
    if !auth_user.can_view_all_payroll() {
        return (axum::http::StatusCode::FORBIDDEN, "給与処理の権限がありません").into_response();
    }

    let year_month = form.year_month;
    let calculator = PayrollCalculator::new(&pool);

    // 有効な全社員を取得
    let employees = payroll_repo::list_employees(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("payroll: fetch_all failed: {:?}", e); vec![] });

    let mut calculated = 0i64;

    for emp in &employees {
        // 既存の給与データがあればスキップ（再計算は別途対応）
        let exists = payroll_repo::payroll_exists(&pool, emp.id, year_month).await.unwrap_or(true);

        if exists {
            continue;
        }

        // 稼働報告を取得（あれば）
        let timesheet = payroll_repo::find_timesheet_for_payroll(&pool, emp.id, year_month).await.ok().flatten();

        let result = calculator.calculate(emp, year_month, timesheet.as_ref()).await;

        match result {
            Ok(data) => {
                // 社員単位の失敗は他の社員の計算を妨げない意図的な部分成功扱い（ログを残し次の社員へ継続）
                match payroll_repo::upsert_payroll(&pool, &data).await {
                    Ok(_) => calculated += 1,
                    Err(e) => tracing::error!("給与保存エラー: {} ({})", emp.full_name(), e),
                }
            }
            Err(e) => {
                tracing::warn!("給与計算エラー: {} ({})", emp.full_name(), e);
            }
        }
    }

    tracing::info!("給与一括計算: {}件（{}分）", calculated, year_month);
    Redirect::to("/payroll").into_response()
}

/// POST /payroll/{id}/confirm — 給与確認
pub async fn confirm(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if !auth_user.can_view_all_payroll() {
        return (axum::http::StatusCode::FORBIDDEN, "給与処理の権限がありません").into_response();
    }

    if let Err(e) = apply_paid_leave_snapshot(&pool, id).await {
        tracing::error!("有給休暇処理エラー: {:?}", e);
    }

    if let Err(e) = payroll_repo::confirm_payroll(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/payroll/{}", id)).into_response()
}

/// POST /payroll/{id}/paid — 振込済
pub async fn paid(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if !auth_user.can_view_all_payroll() {
        return (axum::http::StatusCode::FORBIDDEN, "給与処理の権限がありません").into_response();
    }

    if let Err(e) = payroll_repo::mark_payroll_paid(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/payroll/{}", id)).into_response()
}

// SPA用 JSON API
#[derive(serde::Serialize)]
pub struct PayrollApiResponse {
    pub payrolls: Vec<PayrollWithEmployee>,
    pub summary: PayrollSummary,
}

pub async fn api_index(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Query(filter): Query<PayrollFilter>,
) -> axum::Json<PayrollApiResponse> {
    let month = filter.month.unwrap_or_default();

    // 一般社員は自分自身の給与データのみ閲覧可能（can_view_all_payroll権限者は全件）
    let own_employee_id: Option<i64> = if auth_user.can_view_all_payroll() { None } else { auth_user.employee_id() };

    let payrolls = payroll_repo::list_payrolls_filtered(&pool, &month, own_employee_id)
        .await
        .unwrap_or_else(|e| { tracing::warn!("payroll api: {:?}", e); vec![] });

    let summary = PayrollSummary {
        total_count: payrolls.len() as i64,
        total_gross: payrolls.iter().map(|p| p.gross_pay as i64).sum(),
        total_deduction: payrolls.iter().map(|p| p.deduction_total as i64).sum(),
        total_net: payrolls.iter().map(|p| p.net_pay as i64).sum(),
    };

    axum::Json(PayrollApiResponse { payrolls, summary })
}

/// GET /api/payroll/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let pay = payroll_repo::find_payroll(&pool, id).await.ok().flatten();

    match pay {
        Some(pay) => {
            // 一般社員は自分自身の給与明細のみ閲覧可能（can_view_all_payroll権限者は全件）
            if !auth_user.can_view_all_payroll() && auth_user.employee_id() != Some(pay.employee_id) {
                return (axum::http::StatusCode::FORBIDDEN,
                    axum::Json(serde_json::json!({"error": "他の社員の給与明細は閲覧できません"}))).into_response();
            }

            let employee_name = payroll_repo::find_employee_full_name(&pool, pay.employee_id).await.unwrap_or_default();
            let employee_code = payroll_repo::find_employee(&pool, pay.employee_id).await.ok().flatten()
                .map(|e| e.employee_id).unwrap_or_default();
            let company_name = crate::infrastructure::repositories::company_info_repo::get_company_info(&pool).await.ok().flatten()
                .map(|c| c.name).unwrap_or_default();
            let nearest_expiring = paid_leave_repo::nearest_expiring_grant(&pool, pay.employee_id).await.ok().flatten();

            let status = crate::domain::models::payroll::PayrollStatus::from_str(&pay.status);

            axum::Json(serde_json::json!({
                "payroll": pay,
                "employee_name": employee_name,
                "employee_code": employee_code,
                "company_name": company_name,
                "paid_leave_nearest_expiry": nearest_expiring.map(|g| serde_json::json!({
                    "days": g.remaining_days,
                    "expire_date": g.expire_date,
                })),
                "status_display": status.display(),
                "status_badge": status.badge_class(),
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// POST /api/payroll/calculate
/// body: { "month": "2026-05" | "2026-05-01", "employee_codes": ["0001","0002"]? }
pub async fn api_calculate(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    if !auth_user.can_view_all_payroll() {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "給与処理の権限がありません"}))).into_response();
    }

    let month_raw = payload.get("month").and_then(|v| v.as_str())
        .or_else(|| payload.get("target_month").and_then(|v| v.as_str()))
        .unwrap_or_default();
    let year_month = match parse_payroll_month(month_raw) {
        Some(d) => d,
        None => {
            return (axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "success": false,
                    "error": "対象月の形式が不正です（YYYY-MM または YYYY-MM-01）"
                }))).into_response();
        }
    };

    let employee_codes: Option<Vec<String>> = payload
        .get("employee_codes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        });

    let calculator = PayrollCalculator::new(&pool);
    let employees = payroll_repo::list_employees(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("payroll api_calculate: {:?}", e); vec![] });

    let mut calculated = 0i64;
    let mut skipped = 0i64;
    let mut errors: Vec<String> = Vec::new();

    for emp in &employees {
        if let Some(ref codes) = employee_codes {
            if !codes.is_empty() && !codes.iter().any(|c| c == &emp.employee_id) {
                continue;
            }
        }

        let exists = payroll_repo::payroll_exists(&pool, emp.id, year_month).await.unwrap_or(true);
        if exists {
            skipped += 1;
            continue;
        }

        let timesheet = payroll_repo::find_timesheet_for_payroll(&pool, emp.id, year_month)
            .await
            .ok()
            .flatten();

        match calculator.calculate(emp, year_month, timesheet.as_ref()).await {
            Ok(data) => {
                match payroll_repo::upsert_payroll(&pool, &data).await {
                    Ok(_) => calculated += 1,
                    Err(e) => {
                        tracing::error!("給与保存エラー: {} ({})", emp.full_name(), e);
                        errors.push(format!("{}: 保存失敗", emp.full_name()));
                    }
                }
            }
            Err(e) => {
                tracing::warn!("給与計算エラー: {} ({})", emp.full_name(), e);
                errors.push(format!("{}: {}", emp.full_name(), e));
            }
        }
    }

    tracing::info!(
        "給与一括計算(API): calculated={}, skipped={}, month={}",
        calculated, skipped, year_month
    );

    axum::Json(serde_json::json!({
        "success": true,
        "message": format!(
            "{}の給与計算が完了しました（新規{}件 / 既存スキップ{}件）",
            year_month.format("%Y年%m月"), calculated, skipped
        ),
        "calculated": calculated,
        "skipped": skipped,
        "errors": errors,
    })).into_response()
}

fn parse_payroll_month(raw: &str) -> Option<chrono::NaiveDate> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return chrono::NaiveDate::from_ymd_opt(d.year(), d.month(), 1);
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(&format!("{}-01", s), "%Y-%m-%d") {
        return Some(d);
    }
    None
}

/// POST /api/payroll/{id}/confirm
pub async fn api_confirm(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, crate::presentation::api_response::AppError> {
    if !auth_user.can_view_all_payroll() {
        return Ok((axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "給与処理の権限がありません"}))).into_response());
    }

    if let Err(e) = apply_paid_leave_snapshot(&pool, id).await {
        tracing::error!("有給休暇処理エラー: {:?}", e);
        return Ok(axum::Json(serde_json::json!({ "success": false, "error": "有給休暇の計算に失敗しました" })).into_response());
    }

    let rows = payroll_repo::confirm_payroll_simple(&pool, id).await?;
    Ok(axum::Json(serde_json::json!({ "success": rows > 0 })).into_response())
}

/// POST /api/payroll/{id}/recalculate-deductions — 控除のみ再計算して明細を更新
pub async fn api_recalculate_deductions(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, crate::presentation::api_response::AppError> {
    if !auth_user.can_view_all_payroll() {
        return Ok((axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "給与処理の権限がありません"}))).into_response());
    }

    let pay = match payroll_repo::find_payroll(&pool, id).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            return Ok((axum::http::StatusCode::NOT_FOUND,
                axum::Json(serde_json::json!({"success": false, "error": "給与明細が見つかりません"}))).into_response());
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    let emp = match payroll_repo::find_employee(&pool, pay.employee_id).await {
        Ok(Some(e)) => e,
        Ok(None) => {
            return Ok(axum::Json(serde_json::json!({"success": false, "error": "社員マスタが見つかりません"})).into_response());
        }
        Err(e) => {
            return Err(e.into());
        }
    };

    let calculator = PayrollCalculator::new(&pool);
    let data = calculator.recalculate_deductions(&pay, &emp).await?;

    let rows = payroll_repo::update_payroll_deductions(&pool, id, &data).await?;

    if rows > 0 {
        Ok(axum::Json(serde_json::json!({
            "success": true,
            "message": "控除を再計算して保存しました",
            "deductions": {
                "health_premium": data.health_premium,
                "nursing_premium": data.nursing_premium,
                "pension_premium": data.pension_premium,
                "employment_premium": data.employment_premium,
                "social_insurance_total": data.social_insurance_total,
                "income_tax": data.income_tax,
                "resident_tax": data.resident_tax,
                "deduction_total": data.deduction_total,
                "net_pay": data.net_pay,
            }
        })).into_response())
    } else {
        Ok(axum::Json(serde_json::json!({"success": false, "error": "更新対象がありません"})).into_response())
    }
}

/// POST /api/payroll/{id}/paid
pub async fn api_paid(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, crate::presentation::api_response::AppError> {
    if !auth_user.can_view_all_payroll() {
        return Ok((axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "給与処理の権限がありません"}))).into_response());
    }

    let rows = payroll_repo::mark_payroll_paid_simple(&pool, id).await?;
    Ok(axum::Json(serde_json::json!({ "success": rows > 0 })).into_response())
}
