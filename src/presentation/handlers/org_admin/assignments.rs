/// ユーザーの割り当てと変更履歴の管理API。Admin専用。
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{begin, commit, js, load_tree, log_change, ApiErr, ApiResult};
use crate::domain::authz::admin_input::{assignment_error_message, clamp_limit};
use crate::domain::authz::decision::validate_assignment;
use crate::infrastructure::repositories::{authz_repo, org_assignment_repo, org_change_log_repo, role_repo};
use crate::presentation::middleware::role::AuthUser;

/// GET /api/v1/admin/assignments — 全割り当て(ユーザー・組織・ロールの表示名つき)
pub async fn list(State(pool): State<PgPool>) -> ApiResult<Json<Value>> {
    let rows = org_assignment_repo::list_all(&pool).await?;
    Ok(Json(json!({ "assignments": rows })))
}

#[derive(Debug, Deserialize)]
pub struct CreateAssignmentReq {
    pub user_id: i64,
    pub org_unit_id: i64,
    pub role_id: i64,
}

/// POST /api/v1/admin/assignments — 割り当てを作る
///
/// 有効なユーザー・有効な組織・存在するロールのみ。スタンプ型の権限に全社を持つロールは、ルート組織にだけ割り当てられる。
pub async fn create(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateAssignmentReq>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let mut tx = begin(&pool).await?;
    match org_assignment_repo::user_is_active(&mut *tx, req.user_id).await? {
        None => return Err(ApiErr::BadRequest("ユーザーが見つかりません".to_string())),
        Some(false) => return Err(ApiErr::BadRequest("無効なユーザーには割り当てられません".to_string())),
        Some(true) => {}
    }
    let (units, tree) = load_tree(&mut tx).await?;
    let org = units.iter().find(|u| u.id == req.org_unit_id).ok_or_else(|| ApiErr::BadRequest("組織が見つかりません".to_string()))?;
    if !org.is_active {
        return Err(ApiErr::BadRequest("無効な組織には割り当てられません".to_string()));
    }
    // 無効な組織の配下への割り当ては、権限として働かない(authz_repo と同じ規則)ため、作らせない。
    if authz_repo::disabled_org_ids(&units).contains(&req.org_unit_id) {
        return Err(ApiErr::BadRequest("上位の組織が無効になっているため、この組織には割り当てられません".to_string()));
    }
    if role_repo::find(&mut *tx, req.role_id).await?.is_none() {
        return Err(ApiErr::BadRequest("ロールが見つかりません".to_string()));
    }
    let perms = role_repo::list_permissions_for_roles(&mut *tx, &[req.role_id]).await?;
    validate_assignment(&perms, req.org_unit_id, &tree).map_err(|e| ApiErr::BadRequest(assignment_error_message(&e)))?;

    let row = org_assignment_repo::create(&mut tx, req.user_id, req.org_unit_id, req.role_id, auth.user.id).await?;
    log_change(&mut tx, auth.user.id, "assignment", row.id, "create", None, Some(js(&row))).await?;
    commit(tx).await?;
    Ok((StatusCode::CREATED, Json(json!({ "assignment": row }))))
}

/// DELETE /api/v1/admin/assignments/{id} — 割り当てを外す
pub async fn delete(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let mut tx = begin(&pool).await?;
    let before = org_assignment_repo::delete(&mut tx, id).await?.ok_or(ApiErr::NotFound("割り当てが見つかりません"))?;
    log_change(&mut tx, auth.user.id, "assignment", id, "delete", Some(js(&before)), None).await?;
    commit(tx).await?;
    Ok(Json(json!({ "success": true })))
}

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    pub limit: Option<i64>,
}

/// GET /api/v1/admin/org-role-changes?limit= — 変更履歴(新しい順。既定100件、最大500件)
pub async fn changes(State(pool): State<PgPool>, Query(q): Query<ChangesQuery>) -> ApiResult<Json<Value>> {
    let rows = org_change_log_repo::list_recent(&pool, clamp_limit(q.limit)).await?;
    Ok(Json(json!({ "changes": rows })))
}
