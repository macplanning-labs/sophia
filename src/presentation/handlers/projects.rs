/// presentation/handlers/projects.rs — 案件(m_project) 専用ページ用 JSON API
///
/// WS3(2026-07-31): `/masters`の汎用CRUDから案件を切り離し、専用ページ+作成ウィザードに置き換え。
/// 案件の新規作成は必ずウィザード経由(`/api/projects/wizard`)で、単発の作成エンドポイントは無い。
///
/// ## エンドポイント
/// - GET  /api/projects                  — 一覧
/// - GET  /api/projects/wizard/form-data — ウィザード用マスタデータ(クライアント/技術者/パートナー/作業場所)
/// - POST /api/projects/wizard           — 作成ウィザード一括作成(1トランザクション)
/// - GET  /api/projects/{project_id}     — 詳細(紐づく契約の要約含む)
/// - PUT  /api/projects/{project_id}     — 案件情報のみ更新(is_active・締め日設定等。契約は含まない)

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::infrastructure::repositories::order_repo;
use crate::infrastructure::repositories::project_repo::{
    self, EngineerRef, ProjectUpdateInput, WizardClientContractInput, WizardInput,
    WizardPartnerContractInput, WizardProjectInput, WorkLocationRef,
};
use crate::presentation::api_response::AppError;

// ── 稼働報告提出期限の選択肢(旧 master_repo.rs から移設。/masters "projects" エントリ削除に伴う) ──

pub static REPORT_DEADLINE_TYPE_OPTIONS: &[(&str, &str)] = &[
    ("RELATIVE", "月末相対（N営業日前）"),
    ("FIXED_DAY", "当月固定日"),
];

pub static REPORT_DEADLINE_HOLIDAY_RULE_OPTIONS: &[(&str, &str)] = &[
    ("", "（RELATIVEの場合は未使用）"),
    ("PREVIOUS_BUSINESS_DAY", "前営業日"),
    ("NEXT_BUSINESS_DAY", "翌営業日"),
];

fn options_json(options: &[(&str, &str)]) -> serde_json::Value {
    serde_json::json!(options.iter().map(|(value, label)| serde_json::json!({ "value": value, "label": label })).collect::<Vec<_>>())
}

// ══════════════════════════════════════════════════════════
// 一覧・詳細
// ══════════════════════════════════════════════════════════

/// GET /api/projects — 一覧(JSON)
pub async fn api_index(State(pool): State<PgPool>) -> impl IntoResponse {
    let projects = project_repo::list_projects(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("projects api_index: {:?}", e); vec![] });
    Json(projects)
}

/// GET /api/projects/{project_id} — 詳細(JSON。紐づく契約の要約含む)
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(project_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let project = match project_repo::find_project_detail(&pool, &project_id).await {
        Ok(Some(p)) => p,
        Ok(None) => return Ok((axum::http::StatusCode::NOT_FOUND, "not found").into_response()),
        Err(e) => return Err(AppError::from(e)),
    };

    let client_contracts = project_repo::list_client_contract_summaries(&pool, &project_id).await.unwrap_or_default();
    let partner_contracts = project_repo::list_partner_contract_summaries(&pool, &project_id).await.unwrap_or_default();

    Ok(Json(serde_json::json!({
        "project": project,
        "client_contracts": client_contracts,
        "partner_contracts": partner_contracts,
    })).into_response())
}

// ══════════════════════════════════════════════════════════
// 案件情報の更新(単発。ウィザードとは別)
// ══════════════════════════════════════════════════════════

#[derive(Debug, serde::Deserialize)]
pub struct ProjectUpdateBody {
    pub client_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    pub edi_project_alias: Option<String>,
    pub report_deadline_type: String,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    pub report_request_day: Option<i32>,
}

/// PUT /api/projects/{project_id} — 案件情報のみ更新(JSON)
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(project_id): Path<String>,
    Json(body): Json<ProjectUpdateBody>,
) -> Result<impl IntoResponse, AppError> {
    let input = ProjectUpdateInput {
        client_id: body.client_id,
        name: body.name,
        description: body.description.unwrap_or_default(),
        is_active: body.is_active,
        edi_project_alias: body.edi_project_alias.unwrap_or_default().trim().to_string(),
        report_deadline_type: body.report_deadline_type,
        report_deadline_value: body.report_deadline_value,
        report_deadline_holiday_rule: body.report_deadline_holiday_rule,
        report_request_day: body.report_request_day,
    };

    match project_repo::update_project(&pool, &project_id, &input).await {
        Ok(rows) if rows > 0 => Ok(Json(serde_json::json!({ "success": true, "message": "更新しました" })).into_response()),
        Ok(_) => Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "該当する案件が見つかりません"
        }))).into_response()),
        Err(e) => Err(AppError::from(e)),
    }
}

/// DELETE /api/projects/{project_id} — 無効（is_active=false）の案件のみ削除可
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(project_id): Path<String>,
) -> impl IntoResponse {
    let Some(project) = project_repo::find_project_detail(&pool, &project_id).await.ok().flatten() else {
        return (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "該当する案件が見つかりません"
        }))).into_response();
    };

    if project.is_active {
        return (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "有効な案件は削除できません。先に「有効」のチェックを外して保存してください。"
        }))).into_response();
    }

    match project_repo::delete_project(&pool, &project_id).await {
        Ok(rows) if rows > 0 => {
            Json(serde_json::json!({ "success": true, "message": "削除しました" })).into_response()
        }
        Ok(_) => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "該当する案件が見つかりません"
        }))).into_response(),
        Err(e) => {
            tracing::error!("projects api_delete: {:?}", e);

            let is_fk_violation = e.downcast_ref::<sqlx::Error>()
                .and_then(|se| se.as_database_error())
                .and_then(|de| de.constraint())
                .map(|c| c.contains("project_id") || c.contains("m_project"))
                .unwrap_or(false);

            if is_fk_violation {
                return (axum::http::StatusCode::CONFLICT, Json(serde_json::json!({
                    "success": false,
                    "error": "この案件には契約・帳票等が紐づいているため削除できません。先に関連データを削除してください。"
                }))).into_response();
            }

            (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "success": false, "error": "削除に失敗しました"
            }))).into_response()
        }
    }
}

// ══════════════════════════════════════════════════════════
// 作成ウィザード
// ══════════════════════════════════════════════════════════

/// GET /api/projects/wizard/form-data — ウィザード用マスタデータ(JSON)
pub async fn api_wizard_form_data(State(pool): State<PgPool>) -> impl IntoResponse {
    let clients = project_repo::list_client_id_name_options(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("projects wizard form-data (clients): {:?}", e); vec![] });
    let engineers = order_repo::list_all_engineer_id_name_options(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("projects wizard form-data (engineers): {:?}", e); vec![] });
    let partners = order_repo::list_partner_id_name_options(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("projects wizard form-data (partners): {:?}", e); vec![] });
    let work_locations = order_repo::list_work_location_id_name_options(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("projects wizard form-data (work_locations): {:?}", e); vec![] });

    Json(serde_json::json!({
        "clients": clients.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "engineers": engineers.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "partners": partners.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "work_locations": work_locations.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "report_deadline_type_options": options_json(REPORT_DEADLINE_TYPE_OPTIONS),
        "report_deadline_holiday_rule_options": options_json(REPORT_DEADLINE_HOLIDAY_RULE_OPTIONS),
    }))
}

#[derive(Debug, serde::Deserialize)]
pub struct WizardProjectBody {
    pub client_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub report_deadline_type: String,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
    pub report_request_day: Option<i32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WizardClientContractBody {
    pub engineer: EngineerRef,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub settlement_type: String,
    pub base_rate: i32,
    pub effort: Decimal,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub mid_month_rule: Option<String>,
    pub billing_timing: Option<String>,
    pub payment_terms: Option<String>,
    pub report_deadline_days_before: Option<i32>,
    pub currency: Option<String>,
    pub remarks: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WizardPartnerContractBody {
    pub partner_id: String,
    pub engineer: EngineerRef,
    pub work_location: WorkLocationRef,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub settlement_type: String,
    pub base_rate: i32,
    pub effort: Decimal,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub mid_month_rule: Option<String>,
    #[serde(default)]
    pub kou_responsible: Option<String>,
    #[serde(default)]
    pub kou_contact: Option<String>,
    #[serde(default)]
    pub otsu_responsible: Option<String>,
    #[serde(default)]
    pub otsu_contact: Option<String>,
    #[serde(default)]
    pub work_responsible: Option<String>,
    #[serde(default)]
    pub deliverable_text: Option<String>,
    #[serde(default)]
    pub payment_condition: Option<String>,
    #[serde(default)]
    pub contract_items: Option<String>,
    #[serde(default)]
    pub remarks: Option<String>,
    pub order_create_deadline_day: Option<i32>,
    pub order_approve_deadline_days_before: Option<i32>,
    pub report_upload_deadline_days_before: Option<i32>,
    pub invoice_create_deadline_day: Option<i32>,
    pub invoice_approve_deadline_day: Option<i32>,
}

#[derive(Debug, serde::Deserialize)]
pub struct WizardRequestBody {
    pub project: WizardProjectBody,
    /// クライアント契約ステップはスキップ可能(2026-07-31確定) — null/未指定ならスキップ扱い。
    #[serde(default)]
    pub client_contract: Option<WizardClientContractBody>,
    /// パートナー契約は0件可(2026-07-31確定)。
    #[serde(default)]
    pub partner_contracts: Vec<WizardPartnerContractBody>,
}

/// POST /api/projects/wizard — 案件作成ウィザード一括作成(JSON)
pub async fn api_wizard_create(
    State(pool): State<PgPool>,
    Json(body): Json<WizardRequestBody>,
) -> Result<impl IntoResponse, AppError> {
    let input = WizardInput {
        project: WizardProjectInput {
            client_id: body.project.client_id,
            name: body.project.name,
            description: body.project.description.unwrap_or_default(),
            report_deadline_type: body.project.report_deadline_type,
            report_deadline_value: body.project.report_deadline_value,
            report_deadline_holiday_rule: body.project.report_deadline_holiday_rule,
            report_request_day: body.project.report_request_day,
        },
        client_contract: body.client_contract.map(|cc| WizardClientContractInput {
            engineer: cc.engineer,
            start_date: cc.start_date,
            end_date: cc.end_date,
            settlement_type: cc.settlement_type,
            lower_limit_hours: cc.lower_limit_hours,
            upper_limit_hours: cc.upper_limit_hours,
            fixed_hours: cc.fixed_hours,
            base_rate: cc.base_rate,
            deduction_rate: cc.deduction_rate,
            overtime_rate: cc.overtime_rate,
            effort: cc.effort,
            mid_month_rule: cc.mid_month_rule,
            billing_timing: cc.billing_timing,
            payment_terms: cc.payment_terms,
            report_deadline_days_before: cc.report_deadline_days_before,
            currency: cc.currency,
            remarks: cc.remarks,
        }),
        partner_contracts: body.partner_contracts.into_iter().map(|pc| WizardPartnerContractInput {
            partner_id: pc.partner_id,
            engineer: pc.engineer,
            work_location: pc.work_location,
            start_date: pc.start_date,
            end_date: pc.end_date,
            settlement_type: pc.settlement_type,
            lower_limit_hours: pc.lower_limit_hours,
            upper_limit_hours: pc.upper_limit_hours,
            fixed_hours: pc.fixed_hours,
            base_rate: pc.base_rate,
            deduction_rate: pc.deduction_rate,
            overtime_rate: pc.overtime_rate,
            effort: pc.effort,
            mid_month_rule: pc.mid_month_rule,
            kou_responsible: pc.kou_responsible,
            kou_contact: pc.kou_contact,
            otsu_responsible: pc.otsu_responsible,
            otsu_contact: pc.otsu_contact,
            work_responsible: pc.work_responsible,
            deliverable_text: pc.deliverable_text,
            payment_condition: pc.payment_condition,
            contract_items: pc.contract_items,
            remarks: pc.remarks,
            order_create_deadline_day: pc.order_create_deadline_day,
            order_approve_deadline_days_before: pc.order_approve_deadline_days_before,
            report_upload_deadline_days_before: pc.report_upload_deadline_days_before,
            invoice_create_deadline_day: pc.invoice_create_deadline_day,
            invoice_approve_deadline_day: pc.invoice_approve_deadline_day,
        }).collect(),
    };

    match project_repo::create_project_wizard(&pool, input).await {
        Ok(project_id) => Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({
            "success": true, "project_id": project_id
        }))).into_response()),
        Err(e) => Err(AppError::from(e)),
    }
}
