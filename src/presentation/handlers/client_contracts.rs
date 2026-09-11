/// presentation/handlers/client_contracts.rs — 顧客契約 CRUD
///
/// Phase 2: 受注管理 CRUD の一部。
///
/// ## エンドポイント
/// - GET  /client-contracts           — 一覧（フィルタ: project, active, q）
/// - GET  /client-contracts/new       — 新規作成フォーム
/// - POST /client-contracts           — 作成
/// - GET  /client-contracts/{id}      — 詳細
/// - GET  /client-contracts/{id}/edit — 編集フォーム
/// - POST /client-contracts/{id}      — 更新

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::domain::models::client_contract::{ClientContractForm, ClientContractWithNames};
use crate::infrastructure::repositories::order_repo;
use crate::presentation::api_response::AppError;

// ── テンプレート ──

// ── フィルタパラメータ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct ContractFilter {
    pub project: Option<String>,
    pub active: Option<String>,
    pub q: Option<String>,
}

// ── ハンドラ ──

/// POST /client-contracts — 作成
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<ClientContractForm>,
) -> impl IntoResponse {
    let is_active = form.is_active.as_deref() == Some("on");

    let result = order_repo::insert_client_contract(&pool, &form, is_active).await;

    match result {
        Ok(id) => Redirect::to(&format!("/client-contracts/{}", id)),
        Err(e) => {
            tracing::error!("顧客契約作成エラー: {}", e);
            Redirect::to("/client-contracts")
        }
    }
}

/// POST /client-contracts/{id} — 更新
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<ClientContractForm>,
) -> impl IntoResponse {
    let is_active = form.is_active.as_deref() == Some("on");

    if let Err(e) = order_repo::update_client_contract(&pool, id, &form, is_active).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/client-contracts/{}", id))
}

// ── ヘルパー ──

async fn load_form_options(pool: &PgPool) -> (Vec<crate::domain::models::project::ProjectWithClient>, Vec<crate::domain::models::engineer::Engineer>) {
    let projects = order_repo::list_active_projects_with_client(pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("client_contracts: fetch_all failed: {:?}", e); vec![] });

    let engineers = order_repo::list_active_engineers(pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("client_contracts: fetch_all failed: {:?}", e); vec![] });

    (projects, engineers)
}

// SPA用 JSON API
/// GET /api/client-contracts — 一覧（JSON）
pub async fn api_index(State(pool): State<PgPool>) -> axum::Json<Vec<ClientContractWithNames>> {
    let contracts = order_repo::list_client_contracts_with_names(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("client_contracts api: {:?}", e); vec![] });
    axum::Json(contracts)
}

/// ロック判定: 紐づく受注書がACCEPTED以降ならtrue
async fn is_cc_locked(pool: &PgPool, cc_id: i64) -> bool {
    order_repo::count_locked_orders_for_client_contract(pool, cc_id).await.unwrap_or(0) > 0
}

/// GET /api/client-contracts/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let contract = order_repo::find_client_contract_with_names(&pool, id).await.ok().flatten();

    match contract {
        Some(c) => {
            let order_items = order_repo::list_recent_order_items_for_contract(&pool, id).await.unwrap_or_default();

            let is_locked = is_cc_locked(&pool, id).await;
            axum::Json(serde_json::json!({
                "contract": c,
                "order_items": order_items,
                "is_locked": is_locked,
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ══════════════════════════════════════════════════════════
// SPA用 JSON CRUD API
// ══════════════════════════════════════════════════════════

/// POST /api/client-contracts — 作成（JSON）
pub async fn api_create(
    State(pool): State<PgPool>,
    axum::Json(form): axum::Json<ClientContractForm>,
) -> Result<impl IntoResponse, AppError> {
    let is_active = form.is_active.as_deref() == Some("true") || form.is_active.as_deref() == Some("on");

    let id = order_repo::insert_client_contract(&pool, &form, is_active).await?;

    Ok((axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({
        "success": true, "id": id, "message": "作成しました"
    }))).into_response())
}

/// PUT /api/client-contracts/{id} — 更新（JSON）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(form): axum::Json<ClientContractForm>,
) -> Result<impl IntoResponse, AppError> {
    // ロックガード
    if is_cc_locked(&pool, id).await {
        return Ok((axum::http::StatusCode::FORBIDDEN, axum::Json(serde_json::json!({
            "success": false, "error": "承諾済みの受注書が存在するため、この契約は編集できません"
        }))).into_response());
    }

    let is_active = form.is_active.as_deref() == Some("true") || form.is_active.as_deref() == Some("on");

    order_repo::update_client_contract(&pool, id, &form, is_active).await?;
    Ok(axum::Json(serde_json::json!({ "success": true, "message": "更新しました" })).into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct ExtendForm {
    pub end_date: String,
}

/// POST /api/client-contracts/{id}/extend — 契約延長（終了日のみ更新、JSON）
///
/// 承諾済み受注書によるロック（is_cc_locked）の対象外。
/// 既存の受注書は精算条件をスナップショット済みで影響を受けないため、
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

    let current = order_repo::find_client_contract_with_names(&pool, id).await.ok().flatten();
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

    order_repo::extend_client_contract_end_date(&pool, id, new_end_date).await?;

    Ok(axum::Json(serde_json::json!({ "success": true, "message": "契約を延長しました", "end_date": new_end_date })).into_response())
}

/// GET /api/client-contracts/form-data — フォーム用データ（JSON）
pub async fn api_form_data(State(pool): State<PgPool>) -> impl IntoResponse {
    let (projects, engineers) = load_form_options(&pool).await;
    axum::Json(serde_json::json!({
        "projects": projects.iter().map(|p| serde_json::json!({
            "value": p.project_id, "label": format!("{} ({})", p.name, p.client_name)
        })).collect::<Vec<_>>(),
        "engineers": engineers.iter().map(|e| serde_json::json!({
            "value": e.id, "label": &e.name
        })).collect::<Vec<_>>(),
    }))
}

/// DELETE /api/client-contracts/{id}
/// 承諾済み受注書が紐づく契約は削除不可。それ以外は有効／無効を問わず削除可。
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let Some(_contract) = order_repo::find_client_contract_with_names(&pool, id).await.ok().flatten() else {
        return (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "success": false, "error": "該当する受注契約が見つかりません"
        }))).into_response();
    };

    // ロックガード
    if is_cc_locked(&pool, id).await {
        return (axum::http::StatusCode::FORBIDDEN, axum::Json(serde_json::json!({
            "success": false, "error": "承諾済みの受注書が存在するため、この契約は削除できません"
        }))).into_response();
    }

    match order_repo::delete_client_contract(&pool, id).await
    {
        Ok(rows) if rows > 0 => {
            axum::Json(serde_json::json!({ "success": true, "message": "削除しました" })).into_response()
        }
        Ok(_) => {
            (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
                "success": false, "error": "該当する受注契約が見つかりません"
            }))).into_response()
        }
        Err(e) => {
            tracing::error!("api_delete client_contract error: {:?}", e);

            let is_fk_violation = e.downcast_ref::<sqlx::Error>()
                .and_then(|se| se.as_database_error())
                .and_then(|de| de.constraint())
                .map(|c| c.contains("client_contract"))
                .unwrap_or(false);

            if is_fk_violation {
                return (axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
                    "success": false, "error": "この受注契約には受注書等が紐づいているため削除できません。先に関連データを削除してください。"
                }))).into_response();
            }

            (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false, "error": "削除に失敗しました"
            }))).into_response()
        }
    }
}
