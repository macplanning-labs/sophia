/// presentation/handlers/partner_contracts.rs — パートナー契約 CRUD
///
/// Phase 3: パートナー契約マスタの管理。
///
/// ## エンドポイント
/// - GET  /partner-contracts           — 一覧
/// - GET  /partner-contracts/new       — 新規作成フォーム
/// - POST /partner-contracts           — 作成
/// - GET  /partner-contracts/{id}      — 詳細
/// - GET  /partner-contracts/{id}/edit — 編集フォーム
/// - POST /partner-contracts/{id}      — 更新

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;
use rust_decimal::Decimal;

use crate::infrastructure::repositories::order_repo::{self, PartnerContractFormInput, PartnerContractRow};
use crate::presentation::api_response::AppError;

// ── テンプレート ──

// ── ハンドラ ──

/// POST /partner-contracts — 作成
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<ContractForm>,
) -> impl IntoResponse {
    let start_date = chrono::NaiveDate::parse_from_str(&form.start_date, "%Y-%m-%d")
        .unwrap_or(chrono::Local::now().date_naive());
    let end_date = chrono::NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d")
        .unwrap_or(start_date);

    let result = order_repo::insert_partner_contract(&pool, &form.to_input(start_date, end_date)).await;

    match result {
        Ok(id) => Redirect::to(&format!("/partner-contracts/{}", id)),
        Err(_) => Redirect::to("/partner-contracts/new"),
    }
}

/// POST /partner-contracts/{id} — 更新
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<ContractForm>,
) -> impl IntoResponse {
    let start_date = chrono::NaiveDate::parse_from_str(&form.start_date, "%Y-%m-%d")
        .unwrap_or(chrono::Local::now().date_naive());
    let end_date = chrono::NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d")
        .unwrap_or(start_date);

    if let Err(e) = order_repo::update_partner_contract(&pool, id, &form.to_input(start_date, end_date)).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/partner-contracts/{}", id))
}

// ── フォーム ──

#[derive(Debug, serde::Deserialize)]
pub struct ContractForm {
    pub project_id: String,
    pub engineer_id: i64,
    pub partner_id: String,
    pub start_date: String,
    pub end_date: String,
    pub settlement_type: String,
    #[serde(default, deserialize_with = "deserialize_decimal_or_zero")]
    pub lower_limit_hours: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal_or_zero")]
    pub upper_limit_hours: Decimal,
    #[serde(default, deserialize_with = "deserialize_optional_decimal")]
    pub fixed_hours: Option<Decimal>,
    pub base_rate: i32,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    #[serde(default, deserialize_with = "deserialize_decimal_or_zero")]
    pub effort: Decimal,
    #[serde(default)]
    pub mid_month_rule: String,
    // 注文書テンプレート情報
    #[serde(default)]
    pub kou_responsible: String,
    #[serde(default)]
    pub kou_contact: String,
    #[serde(default)]
    pub otsu_responsible: String,
    #[serde(default)]
    pub otsu_contact: String,
    #[serde(default)]
    pub work_responsible: String,
    #[serde(default)]
    pub deliverable_text: String,
    #[serde(default)]
    pub payment_condition: String,
    #[serde(default)]
    pub contract_items: String,
    #[serde(default)]
    pub work_location: String,
    #[serde(default)]
    pub remarks: String,
    // 締め日設定（旧EDI_MP OrderBasicInfo相当）
    #[serde(default = "default_order_create_deadline_day")]
    pub order_create_deadline_day: i32,
    #[serde(default)]
    pub order_approve_deadline_days_before: i32,
    #[serde(default = "default_report_upload_deadline_days_before")]
    pub report_upload_deadline_days_before: i32,
    #[serde(default = "default_invoice_create_deadline_day")]
    pub invoice_create_deadline_day: i32,
    #[serde(default = "default_invoice_approve_deadline_day")]
    pub invoice_approve_deadline_day: i32,
}

impl ContractForm {
    fn to_input(&self, start_date: chrono::NaiveDate, end_date: chrono::NaiveDate) -> PartnerContractFormInput<'_> {
        PartnerContractFormInput {
            project_id: &self.project_id,
            engineer_id: self.engineer_id,
            partner_id: &self.partner_id,
            start_date,
            end_date,
            settlement_type: &self.settlement_type,
            lower_limit_hours: self.lower_limit_hours,
            upper_limit_hours: self.upper_limit_hours,
            fixed_hours: self.fixed_hours,
            base_rate: self.base_rate,
            deduction_rate: self.deduction_rate,
            overtime_rate: self.overtime_rate,
            effort: self.effort,
            mid_month_rule: &self.mid_month_rule,
            kou_responsible: &self.kou_responsible,
            kou_contact: &self.kou_contact,
            otsu_responsible: &self.otsu_responsible,
            otsu_contact: &self.otsu_contact,
            work_responsible: &self.work_responsible,
            deliverable_text: &self.deliverable_text,
            payment_condition: &self.payment_condition,
            contract_items: &self.contract_items,
            work_location: &self.work_location,
            remarks: &self.remarks,
            order_create_deadline_day: self.order_create_deadline_day,
            order_approve_deadline_days_before: self.order_approve_deadline_days_before,
            report_upload_deadline_days_before: self.report_upload_deadline_days_before,
            invoice_create_deadline_day: self.invoice_create_deadline_day,
            invoice_approve_deadline_day: self.invoice_approve_deadline_day,
        }
    }
}

fn default_order_create_deadline_day() -> i32 { 15 }
fn default_report_upload_deadline_days_before() -> i32 { 2 }
fn default_invoice_create_deadline_day() -> i32 { 1 }
fn default_invoice_approve_deadline_day() -> i32 { 10 }

/// 空文字を0としてデシリアライズ（Decimal必須フィールド用）
fn deserialize_decimal_or_zero<'de, D>(deserializer: D) -> Result<Decimal, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(Decimal::ZERO)
    } else {
        s.parse::<Decimal>().map_err(serde::de::Error::custom)
    }
}

/// 空文字をNoneとしてデシリアライズ（Option<Decimal>フィールド用）
fn deserialize_optional_decimal<'de, D>(deserializer: D) -> Result<Option<Decimal>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s: String = serde::Deserialize::deserialize(deserializer)?;
    if s.is_empty() {
        Ok(None)
    } else {
        s.parse::<Decimal>().map(Some).map_err(serde::de::Error::custom)
    }
}

// ── ヘルパー ──

type FormOptions = (Vec<(String, String)>, Vec<(String, String)>, Vec<(i64, String)>, Vec<(i64, String)>);

async fn fetch_form_data(pool: &PgPool) -> FormOptions {
    let partners = order_repo::list_partner_id_name_options(pool).await
        .unwrap_or_else(|e| { tracing::warn!("partner_contracts: fetch_all failed: {:?}", e); vec![] });
    let projects = order_repo::list_project_id_name_options(pool).await
        .unwrap_or_else(|e| { tracing::warn!("partner_contracts: fetch_all failed: {:?}", e); vec![] });
    let engineers = order_repo::list_all_engineer_id_name_options(pool).await
        .unwrap_or_else(|e| { tracing::warn!("partner_contracts: fetch_all failed: {:?}", e); vec![] });
    let work_locations = order_repo::list_work_location_id_name_options(pool).await
        .unwrap_or_else(|e| { tracing::warn!("partner_contracts: fetch_all failed: {:?}", e); vec![] });
    (partners, projects, engineers, work_locations)
}

// ══════════════════════════════════════════════════════════
// SPA用 JSON API
// ══════════════════════════════════════════════════════════

use axum::Json;

/// GET /api/partner-contracts — 一覧（JSON）
pub async fn api_index(State(pool): State<PgPool>) -> Json<Vec<PartnerContractRow>> {
    let contracts = order_repo::list_partner_contract_rows(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("partner_contracts: fetch_all failed: {:?}", e); vec![] });

    Json(contracts)
}

/// ロック判定: 紐づく発注書がACCEPTED以降ならtrue（contract_idで直接判定）
async fn is_contract_id_locked(pool: &PgPool, contract_id: i64) -> bool {
    order_repo::count_locked_orders_for_partner_contract(pool, contract_id).await.unwrap_or(0) > 0
}

/// GET /api/partner-contracts/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let row = order_repo::find_partner_contract_detail(&pool, id).await.ok().flatten();

    match row {
        Some(r) => {
            // ロック判定: 紐づく発注書がACCEPTED以降なら編集不可
            let is_locked = is_contract_id_locked(&pool, r.id).await;

            // 発注履歴
            let order_items: Vec<serde_json::Value> = order_repo::list_order_history_for_partner_contract(&pool, r.id)
            .await.unwrap_or_default()
            .iter().map(|row| serde_json::json!({
                "order_id": row.0, "work_start": row.1, "work_end": row.2,
                "base_fee": row.3, "actual_hours": row.4, "price": row.5,
            })).collect();

            // 支払通知履歴
            let notice_items: Vec<serde_json::Value> = order_repo::list_notice_history_for_partner_contract(&pool, r.id)
            .await.unwrap_or_default()
            .iter().map(|row| serde_json::json!({
                "notice_id": row.0, "target_month": row.1,
                "actual_hours": row.2, "amount": row.3,
            })).collect();

            let mut json = serde_json::to_value(&r).unwrap_or_default();
            json.as_object_mut().map(|m| {
                m.insert("is_locked".to_string(), serde_json::json!(is_locked));
                m.insert("order_items".to_string(), serde_json::json!(order_items));
                m.insert("notice_items".to_string(), serde_json::json!(notice_items));
            });
            Json(json).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// GET /api/partner-contracts/form-data — フォーム用マスタデータ（JSON）
pub async fn api_form_data(State(pool): State<PgPool>) -> impl IntoResponse {
    let (partners, projects, engineers, work_locations) = fetch_form_data(&pool).await;
    Json(serde_json::json!({
        "partners": partners.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "projects": projects.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "engineers": engineers.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
        "work_locations": work_locations.iter().map(|(id, name)| serde_json::json!({ "value": id, "label": name })).collect::<Vec<_>>(),
    }))
}

/// POST /api/partner-contracts — 作成（JSON）
pub async fn api_create(
    State(pool): State<PgPool>,
    Json(form): Json<ContractForm>,
) -> Result<impl IntoResponse, AppError> {
    let start_date = chrono::NaiveDate::parse_from_str(&form.start_date, "%Y-%m-%d")
        .unwrap_or(chrono::Local::now().date_naive());
    let end_date = chrono::NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d")
        .unwrap_or(start_date);

    let id = order_repo::insert_partner_contract(&pool, &form.to_input(start_date, end_date)).await?;

    Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({
        "success": true, "id": id
    }))).into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct ExtendForm {
    pub end_date: String,
}

/// POST /api/partner-contracts/{id}/extend — 契約延長（終了日のみ更新、JSON）
///
/// 承諾済み発注書によるロック（is_contract_id_locked）の対象外。
/// 既存の注文書は精算条件をスナップショット済みで影響を受けないため、
/// 期間の延長（現在の終了日より後ろに伸ばすことのみ）は常に許可する。
pub async fn api_extend(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(form): axum::Json<ExtendForm>,
) -> Result<impl IntoResponse, AppError> {
    let new_end_date = match chrono::NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
            "success": false, "error": "終了日の形式が不正です"
        }))).into_response()),
    };

    let current = order_repo::find_partner_contract_detail(&pool, id).await.ok().flatten();
    let current_end_date = match current {
        Some(c) => c.end_date,
        None => return Ok((axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "success": false, "error": "契約が見つかりません"
        }))).into_response()),
    };

    if new_end_date <= current_end_date {
        return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
            "success": false, "error": "延長後の終了日は現在の終了日より後の日付を指定してください"
        }))).into_response());
    }

    order_repo::extend_partner_contract_end_date(&pool, id, new_end_date).await?;

    Ok(axum::Json(serde_json::json!({ "success": true, "message": "契約を延長しました", "end_date": new_end_date })).into_response())
}

/// PUT /api/partner-contracts/{id} — 更新（JSON）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Json(form): Json<ContractForm>,
) -> Result<impl IntoResponse, AppError> {
    // ロックガード
    if is_contract_id_locked(&pool, id).await {
        return Ok((axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({
            "success": false, "error": "承諾済みの発注書が存在するため、この契約は編集できません"
        }))).into_response());
    }

    let start_date = chrono::NaiveDate::parse_from_str(&form.start_date, "%Y-%m-%d")
        .unwrap_or(chrono::Local::now().date_naive());
    let end_date = chrono::NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d")
        .unwrap_or(start_date);

    order_repo::update_partner_contract(&pool, id, &form.to_input(start_date, end_date)).await?;
    Ok(Json(serde_json::json!({ "success": true, "message": "更新しました" })).into_response())
}

/// DELETE /api/partner-contracts/{id}
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    // ロックガード
    if is_contract_id_locked(&pool, id).await {
        return (axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({
            "success": false, "error": "承諾済みの発注書が存在するため、この契約は削除できません"
        }))).into_response();
    }

    let result = order_repo::delete_partner_contract(&pool, id).await;

    match result {
        Ok(rows) if rows > 0 => {
            Json(serde_json::json!({ "success": true, "message": "削除しました" })).into_response()
        }
        Ok(_) => {
            (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
                "success": false, "error": "該当する発注契約が見つかりません"
            }))).into_response()
        }
        Err(e) => {
            tracing::error!("api_delete partner_contract error: {:?}", e);

            // 発注書明細から参照されている場合、外部キー制約違反の生SQLエラーではなく
            // 分かりやすいメッセージを返す（ロックガードはACCEPTED以降のみ対象のため、
            // DRAFT/SENT段階の発注書が残っている場合はここで初めて検知される）
            let is_fk_violation = e.downcast_ref::<sqlx::Error>()
                .and_then(|se| se.as_database_error())
                .and_then(|de| de.constraint())
                .map(|c| c.contains("partner_contract_id_fkey"))
                .unwrap_or(false);

            if is_fk_violation {
                return (axum::http::StatusCode::CONFLICT, Json(serde_json::json!({
                    "success": false, "error": "この発注契約には発注書が紐づいているため削除できません。先に該当の発注書を削除してください。"
                }))).into_response();
            }

            (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "success": false, "error": "削除に失敗しました"
            }))).into_response()
        }
    }
}
