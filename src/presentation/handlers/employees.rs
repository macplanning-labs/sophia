/// presentation/handlers/employees.rs — 社員管理 CRUD
///
/// Phase 4-D: 社員マスタの管理。
///
/// ## エンドポイント
/// - GET  /employees                — 一覧
/// - GET  /employees/new            — 新規作成フォーム
/// - POST /employees                — 作成
/// - GET  /employees/{id}           — 詳細（給与設定・保険加入状況）
/// - GET  /employees/{id}/edit      — 編集フォーム
/// - POST /employees/{id}           — 更新

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::domain::models::payroll::{Employee, EmployeeForm};
use crate::infrastructure::repositories::employee_repo;

// ── テンプレート ──




// ── ハンドラ ──

/// POST /employees — 作成
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<EmployeeForm>,
) -> impl IntoResponse {
    let result = employee_repo::insert(&pool, &form).await;

    match result {
        Ok(id) => Redirect::to(&format!("/employees/{}", id)),
        Err(_) => Redirect::to("/employees/new"),
    }
}

/// POST /employees/{id} — 更新
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<EmployeeForm>,
) -> impl IntoResponse {
    if let Err(e) = employee_repo::update(&pool, id, &form).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/employees/{}", id))
}

// SPA用 JSON API

/// POST /api/employees — 作成（JSON）
pub async fn api_create(
    State(pool): State<PgPool>,
    axum::Json(form): axum::Json<EmployeeForm>,
) -> impl IntoResponse {
    let result = employee_repo::insert(&pool, &form).await;

    match result {
        Ok(id) => axum::Json(serde_json::json!({ "success": true, "id": id })).into_response(),
        Err(e) => {
            tracing::error!("api_create employee: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
             axum::Json(serde_json::json!({ "success": false, "error": format!("{e}") }))
            ).into_response()
        }
    }
}

pub async fn api_index(State(pool): State<PgPool>) -> axum::Json<Vec<Employee>> {
    let employees = employee_repo::list_all(&pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("employees api: {:?}", e); vec![] });
    axum::Json(employees)
}

/// GET /api/employees/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let emp = employee_repo::find_by_id(&pool, id).await.ok().flatten();

    match emp {
        Some(e) => axum::Json(serde_json::json!(e)).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// PUT /api/employees/{id} — 更新（JSON）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(form): axum::Json<EmployeeForm>,
) -> impl IntoResponse {
    let result = employee_repo::update(&pool, id, &form).await;

    match result {
        Ok(_) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => {
            tracing::error!("api_update employee: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
             axum::Json(serde_json::json!({ "success": false, "error": format!("{e}") }))
            ).into_response()
        }
    }
}

/// DELETE /api/employees/{id} — 論理削除（JSON）
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = employee_repo::deactivate(&pool, id).await;

    match result {
        Ok(_) => {
            tracing::info!("社員論理削除: id={}", id);
            axum::Json(serde_json::json!({ "success": true })).into_response()
        }
        Err(e) => {
            tracing::error!("api_delete employee: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
             axum::Json(serde_json::json!({ "success": false, "error": format!("{e}") }))
            ).into_response()
        }
    }
}
