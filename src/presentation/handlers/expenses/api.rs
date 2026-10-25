/// expenses/api.rs — SPA用 JSON API（経費申請ヘッダーCRUD・承認/差戻し・マスタ選択肢）

use axum::{
    extract::{Extension, Path, State},
    response::IntoResponse,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use crate::domain::authz::Authz;
use crate::domain::models::expense::{ExpenseRequestCreateForm, ExpenseStatus};
use crate::domain::services::expense_access;
use crate::domain::services::document_workflow::{
    self, ActorContext, DocumentKind, WorkflowAction,
};
use crate::infrastructure::repositories::expense_repo::{self, ExpenseCategoryOption, ExpenseRow};
use crate::presentation::middleware::role::AuthUser;
use crate::presentation::api_response::AppError;

/// 操作する人の立場を作る。管理操作の可否は操作ごとに違う(D12: 承認・承認取り消しは管理者のみ、
/// 差戻しは経理・人事にも開放、再申請などはこれまでどおり全件を見られる人)。
fn expense_actor(auth_user: &AuthUser, authz: Option<&Authz>, employee_id: i64, action: WorkflowAction) -> ActorContext {
    let can_manage = match action {
        WorkflowAction::Approve | WorkflowAction::Unapprove | WorkflowAction::MarkPaid => auth_user.can_approve_expense(authz),
        WorkflowAction::Reject => auth_user.can_review_expense(authz),
        WorkflowAction::Submit | WorkflowAction::Resubmit => auth_user.can_see_all_expenses(authz),
    };
    ActorContext {
        user_id: auth_user.user.id,
        can_manage,
        is_owner: auth_user.employee_id() == Some(employee_id),
    }
}

async fn run_expense_transition(
    pool: &sqlx::PgPool,
    auth_user: &AuthUser,
    authz: Option<&Authz>,
    id: i64,
    action: WorkflowAction,
) -> Result<document_workflow::TransitionPlan, (axum::http::StatusCode, axum::Json<serde_json::Value>)> {
    let Some(exp) = expense_repo::find_by_id(pool, id).await.log_err().ok().flatten() else {
        return Err((
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"success": false, "error": "経費申請が見つかりません"})),
        ));
    };

    let actor = expense_actor(auth_user, authz, exp.employee_id, action);
    if !actor.can_manage && !actor.is_owner {
        return Err((
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "この経費申請を操作する権限がありません"})),
        ));
    }

    let plan = match document_workflow::evaluate_transition(
        DocumentKind::ExpenseRequest,
        &exp.status,
        action,
        &actor,
    ) {
        Ok(p) => p,
        Err(e) => {
            let msg = e.to_string();
            let status = if msg.contains("権限") {
                axum::http::StatusCode::FORBIDDEN
            } else {
                axum::http::StatusCode::BAD_REQUEST
            };
            return Err((status, axum::Json(serde_json::json!({"success": false, "error": msg}))));
        }
    };

    let approved_by = if plan.to_status == "APPROVED" {
        Some(auth_user.user.id)
    } else {
        None
    };

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"success": false, "error": format!("DBエラー: {e}")})),
            ));
        }
    };

    if let Err(e) = expense_repo::apply_workflow_status_tx(
        &mut tx,
        id,
        &plan.from_status,
        &plan.to_status,
        approved_by,
    )
    .await
    {
        return Err((
            axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": e.to_string()})),
        ));
    }

    if let Err(e) = document_workflow::write_transition_log(
        &mut tx,
        DocumentKind::ExpenseRequest,
        &id.to_string(),
        &plan,
        &actor,
    )
    .await
    {
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"success": false, "error": format!("遷移ログの記録に失敗しました: {e}")})),
        ));
    }

    if let Err(e) = crate::infrastructure::db_tx::commit_checked(tx).await {
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"success": false, "error": format!("コミットに失敗しました: {e}")})),
        ));
    }

    Ok(plan)
}

/// GET /api/expenses — 一覧
///
/// 一般社員は自分自身の申請のみ。管理者 / can_view_all_expenses 権限者は全件。
pub async fn api_index(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
) -> axum::Json<Vec<ExpenseRow>> {
    let authz = authz.map(|e| e.0);
    let own_employee_id = if auth_user.can_see_all_expenses(authz.as_ref()) { None } else { auth_user.employee_id() };

    let rows = expense_repo::list_expense_rows(&pool, own_employee_id)
        .await
        .unwrap_or_else(|e| { tracing::warn!("expenses api: {:?}", e); vec![] });
    axum::Json(rows)
}

/// GET /api/expenses/{id} — 詳細（ヘッダー + 明細一覧）
///
/// 一般社員は自分自身の申請のみ。管理者 / can_view_all_expenses 権限者は全件。
pub async fn api_detail(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let expense = expense_repo::find_by_id(&pool, id).await.log_err().ok().flatten();

    match expense {
        Some(exp) => {
            if !auth_user.can_see_all_expenses(authz.as_ref().map(|e| &e.0)) && auth_user.employee_id() != Some(exp.employee_id) {
                return (axum::http::StatusCode::FORBIDDEN,
                    axum::Json(serde_json::json!({"error": "他の社員の経費申請は閲覧できません"}))).into_response();
            }

            let employee_name = expense_repo::find_employee_full_name(&pool, exp.employee_id)
                .await.log_err().unwrap_or_default();

            let items = expense_repo::list_items(&pool, id).await.log_err().unwrap_or_default();
            let categories = expense_repo::list_category_options(&pool).await.log_err().unwrap_or_default();

            let items_json: Vec<serde_json::Value> = items.iter().map(|item| {
                let category_display = categories.iter()
                    .find(|c| c.code == item.category)
                    .map(|c| c.name.clone())
                    .unwrap_or_else(|| item.category.clone());
                serde_json::json!({
                    "id": item.id,
                    "expense_request_id": item.expense_request_id,
                    "expense_date": item.expense_date,
                    "category": item.category,
                    "category_display": category_display,
                    "description": item.description,
                    "amount": item.amount,
                    "has_receipt": item.has_receipt,
                    "receipt_mime": item.receipt_mime,
                    "display_order": item.display_order,
                })
            }).collect();

            let status = ExpenseStatus::from_str(&exp.status);

            axum::Json(serde_json::json!({
                "expense": exp,
                "items": items_json,
                "employee_name": employee_name,
                "status_display": status.display(),
                "status_badge": status.badge_class(),
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// POST /api/expenses — 新規作成（ヘッダー + 初期明細）
///
/// 全件権限者（管理者・経理）は他の社員の代理入力ができる。それ以外は申請者を自分に限る。
pub async fn api_create(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    axum::Json(form): axum::Json<ExpenseRequestCreateForm>,
) -> Result<impl IntoResponse, AppError> {
    if form.employee_id <= 0 {
        return Ok((axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "社員を選択してください"}))).into_response());
    }

    if !expense_access::can_access(auth_user.can_see_all_expenses(authz.as_ref().map(|e| &e.0)), auth_user.employee_id(), form.employee_id) {
        return Ok(super::items::expense_forbidden_response());
    }

    let (id, item_ids) = expense_repo::insert_pending(&pool, form.employee_id, &form.items).await?;
    Ok((axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({
        "success": true, "id": id, "item_ids": item_ids
    }))).into_response())
}

#[derive(Debug, serde::Deserialize)]
pub struct ApiExpenseUpdateForm {
    pub employee_id: i64,
}

/// PUT /api/expenses/{id} — 申請者（担当社員）の変更（PENDINGのもののみ。明細の編集は`items`側のAPIを使う）
pub async fn api_update(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
    axum::Json(form): axum::Json<ApiExpenseUpdateForm>,
) -> impl IntoResponse {
    if form.employee_id <= 0 {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "申請者を選択してください"}))).into_response();
    }

    let Some(exp) = expense_repo::find_by_id(&pool, id).await.log_err().ok().flatten() else {
        return (axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"success": false, "error": "経費申請が見つかりません"}))).into_response();
    };

    let can_see_all = auth_user.can_see_all_expenses(authz.as_ref().map(|e| &e.0));
    if !can_see_all && auth_user.employee_id() != Some(exp.employee_id) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "他の社員の経費申請は編集できません"}))).into_response();
    }

    // 申請者を別人へ付け替えるのは全件閲覧権限者のみ（一般社員は自分の申請のまま保存する用途）
    if form.employee_id != exp.employee_id && !can_see_all {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({"success": false, "error": "申請者の変更権限がありません"}))).into_response();
    }

    if ExpenseStatus::from_str(&exp.status) != ExpenseStatus::Pending {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "申請中の経費のみ申請者を変更できます"}))).into_response();
    }

    match expense_repo::update_employee(&pool, id, form.employee_id).await {
        Ok(rows) if rows > 0 => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Ok(_) => (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"success": false, "error": "申請者を更新できませんでした"}))).into_response(),
        Err(e) => {
            tracing::warn!("expenses api_update: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "success": false, "error": "申請者の更新に失敗しました" }))).into_response()
        }
    }
}

/// DELETE /api/expenses/{id}
pub async fn api_delete(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if let Err(resp) = super::items::check_header_access(&pool, &auth_user, authz.as_ref().map(|e| &e.0), id).await {
        return Ok(resp);
    }
    let rows = expense_repo::delete_pending(&pool, id).await?;
    Ok(axum::Json(serde_json::json!({ "success": rows > 0 })).into_response())
}

/// POST /api/expenses/{id}/approve
pub async fn api_approve(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match run_expense_transition(&pool, &auth_user, authz.as_ref().map(|e| &e.0), id, WorkflowAction::Approve).await {
        Ok(plan) => axum::Json(serde_json::json!({
            "success": true,
            "from_status": plan.from_status,
            "to_status": plan.to_status,
        })).into_response(),
        Err(resp) => resp.into_response(),
    }
}

/// POST /api/expenses/{id}/reject
pub async fn api_reject(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match run_expense_transition(&pool, &auth_user, authz.as_ref().map(|e| &e.0), id, WorkflowAction::Reject).await {
        Ok(plan) => axum::Json(serde_json::json!({
            "success": true,
            "from_status": plan.from_status,
            "to_status": plan.to_status,
        })).into_response(),
        Err(resp) => resp.into_response(),
    }
}

/// POST /api/expenses/{id}/unapprove — 承認取り消し（APPROVED → PENDING。精算済は不可）
pub async fn api_unapprove(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match run_expense_transition(&pool, &auth_user, authz.as_ref().map(|e| &e.0), id, WorkflowAction::Unapprove).await {
        Ok(plan) => axum::Json(serde_json::json!({
            "success": true,
            "from_status": plan.from_status,
            "to_status": plan.to_status,
        })).into_response(),
        Err(resp) => resp.into_response(),
    }
}

/// POST /api/expenses/{id}/resubmit — 再申請（REJECTED → PENDING）
pub async fn api_resubmit(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    match run_expense_transition(&pool, &auth_user, authz.as_ref().map(|e| &e.0), id, WorkflowAction::Resubmit).await {
        Ok(plan) => axum::Json(serde_json::json!({
            "success": true,
            "from_status": plan.from_status,
            "to_status": plan.to_status,
        })).into_response(),
        Err(resp) => resp.into_response(),
    }
}

/// GET /api/employees/options — 社員選択肢
pub async fn api_employee_options(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let options = expense_repo::list_employee_options(&pool)
        .await.log_err().unwrap_or_else(|e| { tracing::warn!("employee options: {:?}", e); vec![] });
    axum::Json(options)
}

/// GET /api/expenses/categories — 経費科目選択肢（マスタメンテ /masters#expense_categories で管理）
pub async fn api_category_options(
    State(pool): State<PgPool>,
) -> impl IntoResponse {
    let options: Vec<ExpenseCategoryOption> = expense_repo::list_category_options(&pool)
        .await.log_err().unwrap_or_else(|e| { tracing::warn!("expense category options: {:?}", e); vec![] });
    axum::Json(options)
}
