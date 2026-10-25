/// ロール・権限セット・権限カタログの管理API。Admin専用。
use std::collections::HashSet;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{begin, commit, js, load_tree, log_change, ApiErr, ApiResult};
use crate::domain::authz::admin_input::{assignment_error_message, normalize_name, validate_role_code};
use crate::domain::authz::decision::{validate_role_permissions, RolePermissionRow};
use crate::domain::authz::scope::DataScope;
use crate::domain::authz::Permission;
use crate::infrastructure::repositories::role_repo;
use crate::presentation::middleware::role::AuthUser;

/// GET /api/v1/admin/permissions — 権限カタログ(コード・日本語名・ドメイン・スタンプ型か・機微か・選べる範囲)
pub async fn catalog() -> Json<Value> {
    let items: Vec<Value> = Permission::ALL
        .iter()
        .map(|p| {
            let scopes: Vec<&str> = if p.is_stamped() { DataScope::ALL.iter().map(|s| s.to_db()).collect() } else { vec!["GLOBAL"] };
            json!({
                "code": p.code(),
                "label_ja": p.label_ja(),
                "domain": p.domain(),
                "is_stamped": p.is_stamped(),
                "is_sensitive": p.is_sensitive(),
                "allowed_scopes": scopes,
            })
        })
        .collect();
    Json(json!({ "permissions": items }))
}

/// GET /api/v1/admin/roles — ロール一覧(割り当て数つき)
pub async fn list(State(pool): State<PgPool>) -> ApiResult<Json<Value>> {
    let roles = role_repo::list_roles(&pool).await?;
    Ok(Json(json!({ "roles": roles })))
}

#[derive(Debug, Deserialize)]
pub struct CreateRoleReq {
    pub code: String,
    pub name: String,
}

/// POST /api/v1/admin/roles — ロールを作る(権限セットは空から始める)
pub async fn create(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateRoleReq>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let code = validate_role_code(&req.code).map_err(ApiErr::BadRequest)?;
    let name = normalize_name(&req.name, "ロール名").map_err(ApiErr::BadRequest)?;
    let mut tx = begin(&pool).await?;
    let role = role_repo::create_role(&mut tx, &code, &name).await?;
    log_change(&mut tx, auth.user.id, "role", role.id, "create", None, Some(js(&role))).await?;
    commit(tx).await?;
    Ok((StatusCode::CREATED, Json(json!({ "role": role }))))
}

#[derive(Debug, Deserialize)]
pub struct RenameRoleReq {
    pub name: String,
}

/// PUT /api/v1/admin/roles/{id} — 表示名の変更(コードは変えられない。標準ロールの表示名は変更できる)
pub async fn rename(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<RenameRoleReq>,
) -> ApiResult<Json<Value>> {
    let name = normalize_name(&req.name, "ロール名").map_err(ApiErr::BadRequest)?;
    let mut tx = begin(&pool).await?;
    let before = role_repo::find(&mut *tx, id).await?.ok_or(ApiErr::NotFound("ロールが見つかりません"))?;
    let after = role_repo::rename_role(&mut tx, id, &name).await?.ok_or(ApiErr::NotFound("ロールが見つかりません"))?;
    log_change(&mut tx, auth.user.id, "role", id, "update", Some(js(&before)), Some(js(&after))).await?;
    commit(tx).await?;
    Ok(Json(json!({ "role": after })))
}

/// DELETE /api/v1/admin/roles/{id} — 削除(標準ロール・割り当てが残るロールは不可)
pub async fn delete(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let mut tx = begin(&pool).await?;
    let before = role_repo::find(&mut *tx, id).await?.ok_or(ApiErr::NotFound("ロールが見つかりません"))?;
    if before.is_system {
        return Err(ApiErr::BadRequest("標準ロールは削除できません".to_string()));
    }
    if !role_repo::assigned_org_ids(&mut *tx, id).await?.is_empty() {
        return Err(ApiErr::Conflict("割り当てが残っているロールは削除できません。先に割り当てを外してください".to_string()));
    }
    let perms = role_repo::list_permissions_for_roles(&mut *tx, &[id]).await?;
    if !role_repo::delete_role(&mut tx, id).await? {
        return Err(ApiErr::Conflict("ロールを削除できませんでした".to_string()));
    }
    let before_json = json!({ "role": before, "permissions": perms_json(&perms) });
    log_change(&mut tx, auth.user.id, "role", id, "delete", Some(before_json), None).await?;
    commit(tx).await?;
    Ok(Json(json!({ "success": true })))
}

/// GET /api/v1/admin/roles/{id}/permissions — ロールの権限セット
pub async fn get_permissions(State(pool): State<PgPool>, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let role = role_repo::find(&pool, id).await?.ok_or(ApiErr::NotFound("ロールが見つかりません"))?;
    let perms = role_repo::list_permissions_for_roles(&pool, &[id]).await?;
    Ok(Json(json!({ "role": role, "permissions": perms_json(&perms) })))
}

#[derive(Debug, Deserialize)]
pub struct PermissionItem {
    pub permission_code: String,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
pub struct ReplacePermissionsReq {
    pub permissions: Vec<PermissionItem>,
}

/// PUT /api/v1/admin/roles/{id}/permissions — 権限セットを丸ごと置き換える
///
/// 既存の割り当て済みの組織も含めて検証する(後から GLOBAL を足して、課の担当者が全社を見られるのを防ぐ)。
pub async fn replace_permissions(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<ReplacePermissionsReq>,
) -> ApiResult<Json<Value>> {
    let rows = to_rows(id, &req.permissions)?;
    let mut tx = begin(&pool).await?;
    let role = role_repo::find(&mut *tx, id).await?.ok_or(ApiErr::NotFound("ロールが見つかりません"))?;
    let assigned = role_repo::assigned_org_ids(&mut *tx, id).await?;
    let (_, tree) = load_tree(&mut tx).await?;
    validate_role_permissions(&rows, &assigned, &tree).map_err(|e| ApiErr::BadRequest(assignment_error_message(&e)))?;

    let before = role_repo::list_permissions_for_roles(&mut *tx, &[id]).await?;
    role_repo::replace_permissions(&mut tx, id, &rows).await?;
    let before_json = json!({ "role_code": role.code, "permissions": perms_json(&before) });
    let after_json = json!({ "role_code": role.code, "permissions": perms_json(&rows) });
    log_change(&mut tx, auth.user.id, "role_permission", id, "update", Some(before_json), Some(after_json)).await?;
    commit(tx).await?;
    Ok(Json(json!({ "role": role, "permissions": perms_json(&rows) })))
}

/// リクエストの権限一覧を検証用の行にする。同じ権限コードの重複は400。
fn to_rows(role_id: i64, items: &[PermissionItem]) -> ApiResult<Vec<RolePermissionRow>> {
    let mut seen = HashSet::new();
    let mut rows = Vec::with_capacity(items.len());
    for it in items {
        if !seen.insert(it.permission_code.as_str()) {
            return Err(ApiErr::BadRequest(format!("権限コードが重複しています: {}", it.permission_code)));
        }
        rows.push(RolePermissionRow { role_id, permission_code: it.permission_code.clone(), scope: it.scope.clone() });
    }
    Ok(rows)
}

fn perms_json(rows: &[RolePermissionRow]) -> Vec<Value> {
    rows.iter().map(|r| json!({ "permission_code": r.permission_code, "scope": r.scope })).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(code: &str, scope: &str) -> PermissionItem {
        PermissionItem { permission_code: code.to_string(), scope: scope.to_string() }
    }

    #[test]
    fn duplicate_permission_codes_are_rejected() {
        assert!(to_rows(1, &[item("contract:view", "UNIT"), item("contract:view", "GLOBAL")]).is_err());
        let rows = to_rows(7, &[item("contract:view", "UNIT"), item("contract:edit", "UNIT")]).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.role_id == 7));
    }
}
