/// 組織の管理API(一覧・作成・更新・付け替え・有効/無効)。Admin専用。
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{begin, commit, js, load_tree, log_change, ApiErr, ApiResult};
use crate::domain::authz::admin_input::{normalize_name, normalize_unit_kind, org_tree_error_message};
use crate::domain::authz::org_tree::OrgTree;
use crate::infrastructure::repositories::org_repo::{self, OrgUnitRow};
use crate::presentation::middleware::role::AuthUser;

/// GET /api/v1/admin/org-units — 全組織(無効も含む)
pub async fn list(State(pool): State<PgPool>) -> ApiResult<Json<Value>> {
    let rows = org_repo::list_all(&pool).await?;
    Ok(Json(json!({ "org_units": rows })))
}

#[derive(Debug, Deserialize)]
pub struct CreateOrgReq {
    pub parent_id: Option<i64>,
    pub name: String,
    pub unit_kind: Option<String>,
    pub sort_order: Option<i32>,
}

/// POST /api/v1/admin/org-units — 組織を作る(ルートは1つだけなので、親は必須)
pub async fn create(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Json(req): Json<CreateOrgReq>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let name = normalize_name(&req.name, "組織名").map_err(ApiErr::BadRequest)?;
    let kind = normalize_unit_kind(req.unit_kind.as_deref()).map_err(ApiErr::BadRequest)?;
    let Some(parent_id) = req.parent_id else {
        return Err(ApiErr::BadRequest("ルート組織は既にあります。親の組織を指定してください".to_string()));
    };
    let mut tx = begin(&pool).await?;
    let (units, _) = load_tree(&mut tx).await?;
    let parent = units.iter().find(|u| u.id == parent_id).ok_or_else(|| ApiErr::BadRequest("親の組織が見つかりません".to_string()))?;
    if !parent.is_active {
        return Err(ApiErr::BadRequest("無効な組織の下には作れません".to_string()));
    }
    // 追加した後の木で、深さの上限を確かめる(仮のIDで試す)。
    let mut pairs: Vec<(i64, Option<i64>)> = units.iter().map(|u| (u.id, u.parent_id)).collect();
    pairs.push((i64::MAX, Some(parent_id)));
    OrgTree::new(&pairs).map_err(|e| ApiErr::BadRequest(org_tree_error_message(&e)))?;

    let row = org_repo::create(&mut tx, Some(parent_id), &name, kind.as_deref(), req.sort_order.unwrap_or(0)).await?;
    log_change(&mut tx, auth.user.id, "org_unit", row.id, "create", None, Some(js(&row))).await?;
    commit(tx).await?;
    Ok((StatusCode::CREATED, Json(json!({ "org_unit": row }))))
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrgReq {
    pub name: String,
    pub unit_kind: Option<String>,
    /// 省略時は今の値のまま。
    pub sort_order: Option<i32>,
}

/// PUT /api/v1/admin/org-units/{id} — 名前・種別・表示順
pub async fn update(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<UpdateOrgReq>,
) -> ApiResult<Json<Value>> {
    let name = normalize_name(&req.name, "組織名").map_err(ApiErr::BadRequest)?;
    let kind = normalize_unit_kind(req.unit_kind.as_deref()).map_err(ApiErr::BadRequest)?;
    let mut tx = begin(&pool).await?;
    let before = find_or_404(&mut tx, id).await?;
    let sort = req.sort_order.unwrap_or(before.sort_order);
    let after = org_repo::update(&mut tx, id, &name, kind.as_deref(), sort).await?.ok_or(ApiErr::NotFound("組織が見つかりません"))?;
    log_change(&mut tx, auth.user.id, "org_unit", id, "update", Some(js(&before)), Some(js(&after))).await?;
    commit(tx).await?;
    Ok(Json(json!({ "org_unit": after })))
}

#[derive(Debug, Deserialize)]
pub struct MoveOrgReq {
    pub new_parent_id: i64,
}

/// POST /api/v1/admin/org-units/{id}/move — 親の付け替え(循環・ルート移動・深さ超過は400)
pub async fn move_unit(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<MoveOrgReq>,
) -> ApiResult<Json<Value>> {
    let mut tx = begin(&pool).await?;
    let (units, tree) = load_tree(&mut tx).await?;
    let before = units.iter().find(|u| u.id == id).cloned().ok_or(ApiErr::NotFound("組織が見つかりません"))?;
    tree.can_reparent(id, req.new_parent_id).map_err(|e| ApiErr::BadRequest(org_tree_error_message(&e)))?;
    if before.parent_id == Some(req.new_parent_id) {
        return Err(ApiErr::BadRequest("既にその組織の下にあります".to_string()));
    }
    if units.iter().any(|u| u.id == req.new_parent_id && !u.is_active) {
        return Err(ApiErr::BadRequest("無効な組織の下には付け替えられません".to_string()));
    }
    let after = org_repo::move_to(&mut tx, id, req.new_parent_id).await?.ok_or(ApiErr::NotFound("組織が見つかりません"))?;
    log_change(&mut tx, auth.user.id, "org_unit", id, "move", Some(js(&before)), Some(js(&after))).await?;
    commit(tx).await?;
    Ok(Json(json!({ "org_unit": after })))
}

#[derive(Debug, Deserialize)]
pub struct ActiveReq {
    pub is_active: bool,
}

/// POST /api/v1/admin/org-units/{id}/active — 有効/無効(ルートは無効にできない。変化がなければ何もしない)
pub async fn set_active(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<ActiveReq>,
) -> ApiResult<Json<Value>> {
    let mut tx = begin(&pool).await?;
    let (units, tree) = load_tree(&mut tx).await?;
    let before = units.iter().find(|u| u.id == id).cloned().ok_or(ApiErr::NotFound("組織が見つかりません"))?;
    if !req.is_active && id == tree.root_id() {
        return Err(ApiErr::BadRequest("ルート組織は無効にできません".to_string()));
    }
    if before.is_active == req.is_active {
        return Ok(Json(json!({ "org_unit": before })));
    }
    let after = org_repo::set_active(&mut tx, id, req.is_active).await?.ok_or(ApiErr::NotFound("組織が見つかりません"))?;
    log_change(&mut tx, auth.user.id, "org_unit", id, "update", Some(js(&before)), Some(js(&after))).await?;
    commit(tx).await?;
    Ok(Json(json!({ "org_unit": after })))
}

async fn find_or_404(conn: &mut sqlx::PgConnection, id: i64) -> ApiResult<OrgUnitRow> {
    org_repo::find(conn, id).await?.ok_or(ApiErr::NotFound("組織が見つかりません"))
}
