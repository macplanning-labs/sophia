/// 自動取込の受け皿(会社設定。D16)の読み書きAPI。Admin専用。DEMO-000173 PR 6c。
///
/// EDI 同期・メール取込で作るデータに、親の担当が無いとき使う「既定の担当組織と担当者」。
/// PR 7b(担当の必須化)の後は、受け皿が未設定だと、親の担当が無い自動取込の行は作られない(エラーログを残して拒否)。
/// そのため、設定は両方必須で、空には戻せない(PUT は組織・担当者の両方を求める)。
use axum::{extract::State, Extension, Json};
use serde_json::{json, Value};
use sqlx::PgPool;

use super::transfer::{validate_target, ToReq};
use super::{begin, commit, log_change, ApiResult};
use crate::infrastructure::repositories::{owner_transfer_repo, stamp_repo};
use crate::presentation::middleware::role::AuthUser;

/// GET /api/v1/admin/auto-import-receiver — 今の受け皿(未設定なら null)
pub async fn get(State(pool): State<PgPool>) -> ApiResult<Json<Value>> {
    let s = stamp_repo::auto_import_receiver(&pool).await?;
    Ok(Json(json!({ "org_unit_id": s.org_unit_id, "employee_id": s.employee_id })))
}

/// PUT /api/v1/admin/auto-import-receiver — 受け皿を設定する(有効な組織・有効な社員のみ。両方必須)
pub async fn put(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Json(req): Json<ToReq>) -> ApiResult<Json<Value>> {
    let mut tx = begin(&pool).await?;
    let before = stamp_repo::auto_import_receiver(&mut *tx).await?;
    let to = validate_target(&mut tx, &req).await?;
    owner_transfer_repo::set_auto_import_receiver(&mut tx, &to).await?;
    let b = json!({ "org_unit_id": before.org_unit_id, "employee_id": before.employee_id });
    let a = json!({ "org_unit_id": to.org_unit_id, "employee_id": to.employee_id });
    log_change(&mut tx, auth.user.id, "auto_import_receiver", 0, "update", Some(b), Some(a.clone())).await?;
    commit(tx).await?;
    Ok(Json(a))
}
