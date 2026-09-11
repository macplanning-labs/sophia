/// presentation/handlers/timesheets.rs — 稼働報告 CRUD + 承認
///
/// Phase 4-B: 稼働報告管理。
///
/// ## エンドポイント
/// - GET  /timesheets                    — 一覧（月別・ステータスフィルタ）
/// - GET  /timesheets/{id}               — 詳細（承認操作付き）
/// - POST /timesheets/{id}/approve       — 承認
/// - POST /timesheets/{id}/reject        — 差戻し
/// - POST /timesheets/{id}/send          — クライアント送付

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use sqlx::PgPool;

use crate::domain::models::timesheet::TimesheetStatus;
use crate::infrastructure::repositories::timesheet_repo::{self, TimesheetRow};
use crate::presentation::handlers::timesheet_upload_common::{self, TimesheetUploadError};

// ── テンプレート ──

/// サマリー
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TimesheetSummary {
    pub total: i64,
    pub pending: i64,
    pub uploaded: i64,
    pub approved: i64,
}

// ── フィルタ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct TimesheetFilter {
    pub status: Option<String>,
    pub month: Option<String>,
}

// ── ハンドラ ──

/// POST /api/timesheets/{id}/approve — 承認（JSON。旧SSR RedirectはSPAのfetch followで404になる）
pub async fn approve(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match timesheet_repo::approve(&pool, id).await {
        Ok(0) => (
            StatusCode::CONFLICT,
            axum::Json(serde_json::json!({
                "success": false,
                "error": "受領済または解析済の稼働報告のみ承認できます"
            })),
        )
            .into_response(),
        Ok(_) => {
            // 案件非依存の社員自己申告（client_contract_id無し）は対象外。
            // クライアント案件向けの確定通知メールのため、案件に紐づく稼働報告のみ送信する
            let is_self_report = timesheet_repo::find_by_id(&pool, id)
                .await
                .ok()
                .flatten()
                .map(|ts| ts.client_contract_id.is_none())
                .unwrap_or(false);

            // EDI互換: 稼働報告確定通知メール（report_approved）。案件非依存の社員自己申告は対象外
            if !is_self_report {
                tokio::spawn({
                    let pool = pool.clone();
                    async move {
                        let info = timesheet_repo::find_report_info_for_notification(&pool, id)
                            .await
                            .ok()
                            .flatten();

                        if let Some(info) = info {
                            let display_name = info.partner_name.unwrap_or_else(|| "不明".into());
                            let month_display = info
                                .work_month
                                .map(|d| d.format("%Y年%m月").to_string())
                                .unwrap_or_else(|| "不明".into());
                            let report_lines = format!(
                                "  ・{}: {}h / {}日",
                                info.worker_name.as_deref().unwrap_or("氏名不明"),
                                info.total_hours.unwrap_or(0.0),
                                info.work_days.unwrap_or(0)
                            );

                            let notify_email =
                                crate::domain::services::email_service::get_notify_email(&pool).await;

                            let ctx = crate::domain::services::email_service::compose_report_approved_email(
                                &display_name,
                                &month_display,
                                "admin",
                                &report_lines,
                            );
                            let email_svc =
                                crate::domain::services::email_service::EmailService::new(pool.clone());
                            if let Err(e) = email_svc
                                .send_by_template("report_approved", &notify_email, None, &ctx)
                                .await
                            {
                                tracing::error!("[稼働報告確定通知] メール送信エラー: {:?}", e);
                            } else {
                                tracing::info!(
                                    "[稼働報告確定通知] {} → {}",
                                    display_name,
                                    notify_email
                                );
                            }
                        }
                    }
                });
            }

            axum::Json(serde_json::json!({ "success": true })).into_response()
        }
        Err(e) => {
            tracing::error!("timesheet approve DB error: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "success": false,
                    "error": "承認に失敗しました"
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/timesheets/{id}/reject — 差戻し
pub async fn reject(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match timesheet_repo::reject(&pool, id).await {
        Ok(()) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => {
            tracing::error!("timesheet reject DB error: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "success": false,
                    "error": "差戻しに失敗しました"
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/timesheets/{id}/send — クライアント送付
pub async fn send_to_client(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match timesheet_repo::send_to_client(&pool, id).await {
        Ok(()) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => {
            tracing::error!("timesheet send DB error: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({
                    "success": false,
                    "error": "送信に失敗しました"
                })),
            )
                .into_response()
        }
    }
}

/// POST /timesheets/upload — 稼働報告アップロード（プレビュー用・DB保存しない）
///
/// Excel / 勤務表PDFを解析し、結果をJSONで返す。フロントでプレビュー表示後、
/// ユーザーが「登録」ボタンを押すと /timesheets/confirm で実際に保存する。
pub async fn upload(
    State(pool): State<PgPool>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let (bytes, original_filename) =
        match timesheet_upload_common::extract_upload_file(&mut multipart).await {
            Ok(v) => v,
            Err(e) => return axum::Json(e.to_json()).into_response(),
        };

    let result = match timesheet_upload_common::parse_upload(&bytes, &original_filename) {
        Ok(r) => r,
        Err(e) => return axum::Json(e.to_json()).into_response(),
    };

    let result = match timesheet_upload_common::prepare_admin_preview_result(&pool, result, None).await
    {
        Ok(r) => r,
        Err(e) => return axum::Json(e.to_json()).into_response(),
    };

    axum::Json(timesheet_upload_common::ok_preview_response(
        &result,
        &original_filename,
    ))
    .into_response()
}

/// POST /timesheets/confirm — 稼働報告プレビュー確認後の登録
///
/// プレビュー後、ユーザーが「登録」を押した際に呼ばれる。
/// Excelファイルを再度受け取り、解析→DB保存する。
/// 成功・失敗とも JSON（`status` / `error`）を返す（サイレント Redirect はしない）。
/// 初期ステータスは PARSED（管理者解析済み）。
pub async fn confirm_upload(
    State(pool): State<PgPool>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let (bytes, original_filename, contract_id) =
        match timesheet_upload_common::extract_confirm_fields(&mut multipart).await {
            Ok(v) => v,
            Err(e) => return axum::Json(e.to_json()).into_response(),
        };

    let result = match timesheet_upload_common::parse_upload(&bytes, &original_filename) {
        Ok(r) => r,
        Err(e) => {
            if let TimesheetUploadError::Parse(ref err) = e {
                tracing::warn!("Excel解析失敗: {}", err);
            }
            return axum::Json(e.to_json()).into_response();
        }
    };

    let target_month = timesheet_upload_common::normalize_target_month(&result);

    let resolved_contract_id = match timesheet_upload_common::resolve_admin_contract_id(
        &pool,
        &result.worker_name,
        target_month,
        contract_id,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => return axum::Json(e.to_json()).into_response(),
    };

    if let Err(e) = timesheet_upload_common::ensure_received_order(
        &pool,
        resolved_contract_id,
        target_month,
        &result.worker_name,
    )
    .await
    {
        return axum::Json(e.to_json()).into_response();
    }

    match timesheet_upload_common::upsert_parsed_timesheet(
        &pool,
        resolved_contract_id,
        target_month,
        &result,
        &original_filename,
        "PARSED",
    )
    .await
    {
        Ok(()) => {
            tracing::info!(
                "稼働報告登録成功: {} → {}h / {}日 (作業者: {})",
                original_filename,
                result.total_hours,
                result.work_days,
                result.worker_name
            );
            axum::Json(timesheet_upload_common::ok_confirm_response()).into_response()
        }
        Err(e) => {
            tracing::error!("稼働報告DB保存エラー: {:?}", e);
            axum::Json(serde_json::json!({
                "status": "error",
                "error": "稼働報告の保存に失敗しました"
            }))
            .into_response()
        }
    }
}

// SPA用 JSON API
#[derive(serde::Serialize)]
pub struct TimesheetApiResponse {
    pub timesheets: Vec<TimesheetRow>,
    pub summary: TimesheetSummary,
}

pub async fn api_index(State(pool): State<PgPool>) -> axum::Json<TimesheetApiResponse> {
    let timesheets = timesheet_repo::list_timesheet_rows(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("timesheets api: {:?}", e); vec![] });

    let summary = TimesheetSummary {
        total: timesheets.len() as i64,
        pending: timesheets.iter().filter(|t| t.status == "PENDING").count() as i64,
        uploaded: timesheets.iter().filter(|t| t.status == "UPLOADED").count() as i64,
        approved: timesheets.iter().filter(|t| t.status == "APPROVED").count() as i64,
    };

    axum::Json(TimesheetApiResponse { timesheets, summary })
}

/// GET /api/timesheets/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let sheet = timesheet_repo::find_by_id(&pool, id).await.ok().flatten();

    match sheet {
        Some(sheet) => {
            let contract_info = match (sheet.client_contract_id, sheet.employee_id) {
                (Some(cc_id), _) => timesheet_repo::find_contract_display_info(&pool, cc_id).await
                    .unwrap_or_else(|_| "(不明)".to_string()),
                (None, Some(emp_id)) => timesheet_repo::find_employee_display_info(&pool, emp_id).await
                    .unwrap_or_else(|_| "(不明)".to_string()),
                (None, None) => "(不明)".to_string(),
            };

            let status_enum = TimesheetStatus::from_str(&sheet.status);
            let hours_f = rust_decimal::prelude::ToPrimitive::to_f64(&sheet.total_hours).unwrap_or(0.0);
            let hours_check = if let Some(cc_id) = sheet.client_contract_id {
                match timesheet_upload_common::settlement_terms_for_client_contract(&pool, cc_id).await
                {
                    Ok(terms) => {
                        let lower = rust_decimal::prelude::ToPrimitive::to_f64(&terms.lower_limit_hours)
                            .unwrap_or(0.0);
                        let upper = rust_decimal::prelude::ToPrimitive::to_f64(&terms.upper_limit_hours)
                            .unwrap_or(0.0);
                        if hours_f < lower {
                            "下限未満"
                        } else if hours_f > upper {
                            "上限超過"
                        } else {
                            "範囲内"
                        }
                    }
                    Err(_) => "—",
                }
            } else {
                "—"
            };

            // 関連メール
            let source_emails: Vec<serde_json::Value> = timesheet_repo::list_source_emails_for_timesheet(&pool, id)
                .await.unwrap_or_default()
            .iter().map(|r| serde_json::json!({
                "from": r.0, "subject": r.1, "received_at": r.2, "status": r.3,
            })).collect();

            axum::Json(serde_json::json!({
                "sheet": sheet,
                "contract_info": contract_info,
                "status_display": status_enum.display(),
                "status_badge": status_enum.badge_class(),
                "hours_check": hours_check,
                "source_emails": source_emails,
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
