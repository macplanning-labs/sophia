use axum::{
    extract::{Query, State},
    Json, response::IntoResponse,
};
use chrono::{NaiveDate, Datelike};
use sqlx::PgPool;
use crate::domain::services::settlement_dashboard::{
    self, SettlementFilter,
};
use crate::presentation::api_response::AppError;
use super::{
    ClientOption, PartnerOption, ProjectOption, DashboardFilter, SettlementApiResponse,
    FiltersApiResponse, MonthOption, ApiIssueForm, generate_month_list,
};

/// GET /api/settlement?month=2026-05-01&client_id=&partner_id=&status=
pub async fn api_index(
    State(pool): State<PgPool>,
    Query(filter): Query<DashboardFilter>,
) -> Result<Json<SettlementApiResponse>, AppError> {
    let today = chrono::Utc::now().date_naive();
    let target_month_str = filter.month.clone().unwrap_or_else(|| {
        format!("{}-{:02}-01", today.year(), today.month())
    });
    let target_month = NaiveDate::parse_from_str(&target_month_str, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", target_month_str), "%Y-%m-%d"))
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap());

    let sfilter = SettlementFilter {
        target_month: Some(target_month_str),
        client_id: filter.client_id,
        partner_id: filter.partner_id.clone(),
        project_id: filter.project_id.clone(),
        status: filter.status.clone(),
    };

    let rows = settlement_dashboard::list_settlement_rows(&pool, target_month, &sfilter).await?;
    let (mut view_rows, summary) = settlement_dashboard::calculate_preview(rows);

    // ステータスフィルタ
    if let Some(ref status) = filter.status {
        view_rows = view_rows.into_iter().filter(|vr| {
            match status.as_str() {
                "pending" => !vr.row.invoice_issued.unwrap_or(false) || !vr.row.notice_issued.unwrap_or(false),
                "invoice_issued" => vr.row.invoice_issued.unwrap_or(false),
                "notice_issued" => vr.row.notice_issued.unwrap_or(false),
                "complete" => vr.row.invoice_issued.unwrap_or(false) && vr.row.notice_issued.unwrap_or(false),
                _ => true,
            }
        }).collect();
    }

    let current_month = target_month.format("%Y年%m月").to_string();

    Ok(Json(SettlementApiResponse {
        rows: view_rows,
        summary,
        current_month,
    }))
}

/// GET /api/settlement/filters — フィルタ用マスタデータ
pub async fn api_filters(
    State(pool): State<PgPool>,
) -> Json<FiltersApiResponse> {
    let today = chrono::Utc::now().date_naive();

    let clients: Vec<ClientOption> = crate::infrastructure::repositories::client_repo::list_id_name_options(&pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(id, name)| ClientOption { id, name })
        .collect();

    let partners: Vec<PartnerOption> = crate::infrastructure::repositories::partner_repo::list_id_name_options(&pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(partner_id, name)| PartnerOption { partner_id, name })
        .collect();

    let projects: Vec<ProjectOption> = crate::infrastructure::repositories::project_repo::list_id_name_options(&pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|(project_id, name)| ProjectOption { project_id, name })
        .collect();

    let months = generate_month_list(today, 6);
    let available_months: Vec<MonthOption> = months.into_iter()
        .map(|(v, l)| MonthOption { value: v, label: l })
        .collect();

    Json(FiltersApiResponse {
        available_months,
        clients,
        partners,
        projects,
    })
}

/// POST /api/settlement/issue-invoices — 請求書一括発行（JSON版）
pub async fn api_issue_invoices(
    State(pool): State<PgPool>,
    Json(form): Json<ApiIssueForm>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_month = NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", form.target_month), "%Y-%m-%d"))
        .unwrap_or_else(|_| chrono::Utc::now().date_naive());

    if form.selected_ids.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "対象が選択されていません",
        })));
    }

    let filter = SettlementFilter::default();
    let rows = settlement_dashboard::list_settlement_rows(&pool, target_month, &filter).await?;
    let (view_rows, _) = settlement_dashboard::calculate_preview(rows);

    let selected_rows: Vec<_> = view_rows.iter()
        .filter(|vr| form.selected_ids.contains(&vr.row.client_contract_id))
        .cloned()
        .collect();

    let edi_excluded_count = selected_rows.iter()
        .filter(|vr| vr.row.client_edi_system_type == "EDI_OASIS")
        .count();

    match settlement_dashboard::create_invoices_by_client(&pool, &selected_rows, target_month).await {
        Ok(results) => {
            if results.is_empty() {
                let blockers = settlement_dashboard::diagnose_invoice_issue_blockers(
                    &pool,
                    &selected_rows,
                    target_month,
                )
                .await;
                let message = if blockers.is_empty() {
                    if selected_rows.is_empty() {
                        "対象行が見つかりませんでした（選択が無効になっている可能性があります）。画面を更新して再度お試しください。".to_string()
                    } else {
                        "請求書を発行できませんでした。管理者に連絡してください。".to_string()
                    }
                } else {
                    use crate::domain::models::actionable_error::format_action_blockers_message;
                    format_action_blockers_message(&blockers, "請求書を発行できませんでした。条件を確認してください。")
                };
                return Ok(Json(serde_json::json!({
                    "success": false,
                    "message": message,
                    "blockers": blockers,
                    "issued_ids": form.selected_ids,
                })));
            }

            let message = if edi_excluded_count > 0 {
                format!(
                    "{}件の請求書を発行しました（EDI連携クライアントの{}件は対象外としてスキップしました）",
                    results.len(), edi_excluded_count
                )
            } else {
                format!("{}件の請求書を発行しました", results.len())
            };
            Ok(Json(serde_json::json!({
                "success": true,
                "message": message,
                "issued_ids": form.selected_ids,
                "created_invoices": results,
            })))
        }
        Err(e) => {
            Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("発行エラー: {}", e),
            })))
        }
    }
}

/// POST /api/settlement/issue-notices — 支払通知書一括発行（JSON版）
pub async fn api_issue_notices(
    State(pool): State<PgPool>,
    Json(form): Json<ApiIssueForm>,
) -> Result<Json<serde_json::Value>, AppError> {
    let target_month = NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", form.target_month), "%Y-%m-%d"))
        .unwrap_or_else(|_| chrono::Utc::now().date_naive());

    if form.selected_ids.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "対象が選択されていません",
        })));
    }

    let filter = SettlementFilter::default();
    let rows = settlement_dashboard::list_settlement_rows(&pool, target_month, &filter).await?;
    let (view_rows, _) = settlement_dashboard::calculate_preview(rows);

    let selected_rows: Vec<_> = view_rows.iter()
        .filter(|vr| {
            vr.row
                .partner_contract_id
                .map_or(false, |id| form.selected_ids.contains(&id))
        })
        .cloned()
        .collect();

    match settlement_dashboard::create_notices_by_partner(&pool, &selected_rows, target_month).await {
        Ok(results) => {
            if results.is_empty() {
                let blockers = settlement_dashboard::diagnose_notice_issue_blockers(
                    &pool,
                    &selected_rows,
                    target_month,
                )
                .await;
                let message = if blockers.is_empty() {
                    if selected_rows.is_empty() {
                        "対象行が見つかりませんでした（選択が無効になっている可能性があります）。画面を更新して再度お試しください。".to_string()
                    } else {
                        "支払通知書を発行できませんでした。管理者に連絡してください。".to_string()
                    }
                } else {
                    settlement_dashboard::format_notice_issue_blockers_message(&blockers)
                };
                return Ok(Json(serde_json::json!({
                    "success": false,
                    "message": message,
                    "blockers": blockers,
                    "issued_ids": form.selected_ids,
                })));
            }
            Ok(Json(serde_json::json!({
                "success": true,
                "message": format!("{}件の支払通知書を発行しました", results.len()),
                "issued_ids": form.selected_ids,
                "created_notices": results,
            })))
        }
        Err(e) => {
            Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("発行エラー: {}", e),
            })))
        }
    }
}

/// POST /api/settlement/import-billings — EDI-OASIS請求書取込
pub async fn api_import_billings(
    State(pool): State<PgPool>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let year = payload["year"].as_i64().unwrap_or(0) as i32;
    let month = payload["month"].as_i64().unwrap_or(0) as i32;

    if year == 0 || month == 0 {
        return Json(serde_json::json!({ "success": false, "message": "year/monthを指定してください" }));
    }

    let client = match crate::infrastructure::edi_oasis_client::EdiOasisClient::from_env() {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({ "success": false, "message": format!("EDI接続エラー: {}", e) })),
    };

    let mut client = client;
    let result = crate::infrastructure::billing_importer::import_billings_for_month(
        &pool, &mut client, year, month,
    ).await;

    Json(serde_json::json!({
        "success": result.errors.is_empty(),
        "message": format!("{}", result),
        "imported": result.billings_imported,
        "skipped": result.billings_skipped,
        "errors": result.errors,
    }))
}
