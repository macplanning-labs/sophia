/// 担当の引き継ぎ(付け替え)の管理API。Admin専用(DEMO-000173 PR 6b。設計書 D15・U11)。
///
/// - `POST /admin/owner-transfer`: 顧客または案件を起点に、配下のデータの担当をまとめて付け替える
/// - `GET  /admin/owner-transfer/by-employee/{id}`: 社員が担当のデータの件数(画面のプレビュー用)
/// - `POST /admin/owner-transfer/by-employee`: 社員が担当のデータすべてを付け替える(退職時の引き継ぎ)
///
/// 社員・ユーザーの無効化も、担当データがあれば `hand_over` で同じ引き継ぎを同じトランザクションの中で行う(D15)。
/// 1操作 = 1トランザクション(ロック → 検証 → 更新 → 履歴 → 確定)。履歴は `h_org_role_change` に1行。
use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool};

use super::{begin, commit, js, ApiErr, ApiResult};
use crate::domain::authz::owner_transfer::{check_not_same_employee, parse_root, parse_to, transfer_input_error_message, TransferRoot};
use crate::domain::authz::stamp::OwnerStamp;
use crate::infrastructure::repositories::owner_transfer_repo::{self as repo, total, TableCount};
use crate::infrastructure::repositories::org_assignment_repo::AssignmentRecord;
use crate::infrastructure::repositories::{authz_repo, employee_repo, org_change_log_repo, org_repo, user_repo};
use crate::presentation::middleware::role::AuthUser;

/// 引き継ぎ先(組織と担当者)。
#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct ToReq {
    pub org_unit_id: Option<i64>,
    pub employee_id: Option<i64>,
}

/// POST /admin/owner-transfer の本文。`client_id` か `project_id` のどちらか一方。
#[derive(Debug, Deserialize)]
pub struct TransferReq {
    pub client_id: Option<i64>,
    pub project_id: Option<String>,
    #[serde(default)]
    pub to: ToReq,
}

/// POST /admin/owner-transfer/by-employee の本文。
#[derive(Debug, Deserialize)]
pub struct ByEmployeeReq {
    pub from_employee_id: i64,
    #[serde(default)]
    pub to: ToReq,
}

/// 社員・ユーザー無効化の本文(任意)。担当データがあるときだけ `transfer_to` が要る。
#[derive(Debug, Deserialize)]
struct DeactivateBody {
    transfer_to: Option<ToReq>,
}

/// 無効化の要求の本文を読む。本文なし(空)は `None`。本文があるのに読めなければ400。
pub fn parse_deactivate_body(bytes: &[u8]) -> ApiResult<Option<ToReq>> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let body: DeactivateBody = serde_json::from_slice(bytes).map_err(|_| ApiErr::BadRequest("リクエストの形式が正しくありません".to_string()))?;
    Ok(body.transfer_to)
}

fn bad_input(e: crate::domain::authz::owner_transfer::TransferInputError) -> ApiErr {
    ApiErr::BadRequest(transfer_input_error_message(&e))
}

fn summary(updated: &[TableCount]) -> Value {
    json!({ "updated": updated, "total": total(updated) })
}

/// 引き継ぎ先を検証して、スタンプにする。組織は存在・有効・上位も有効、担当者は存在・有効。
pub(crate) async fn validate_target(conn: &mut PgConnection, to: &ToReq) -> ApiResult<OwnerStamp> {
    let stamp = parse_to(to.org_unit_id, to.employee_id).map_err(bad_input)?;
    let (org, emp) = (stamp.org_unit_id.unwrap_or_default(), stamp.employee_id.unwrap_or_default());
    let units = org_repo::list_all(&mut *conn).await?;
    let unit = units.iter().find(|u| u.id == org).ok_or_else(|| ApiErr::BadRequest("引き継ぎ先の組織が見つかりません".to_string()))?;
    if !unit.is_active || authz_repo::disabled_org_ids(&units).contains(&org) {
        return Err(ApiErr::BadRequest("無効な組織には引き継げません".to_string()));
    }
    match repo::employee_is_active(&mut *conn, emp).await? {
        None => Err(ApiErr::BadRequest("引き継ぎ先の担当者が見つかりません".to_string())),
        Some(false) => Err(ApiErr::BadRequest("無効な担当者には引き継げません".to_string())),
        Some(true) => Ok(stamp),
    }
}

fn stamp_json(s: &OwnerStamp) -> Value {
    json!({ "org_unit_id": s.org_unit_id, "employee_id": s.employee_id })
}

async fn log_transfer(conn: &mut PgConnection, admin_id: i64, target_id: Option<i64>, before: Value, after: Value) -> ApiResult<()> {
    org_change_log_repo::insert(conn, admin_id, "owner_transfer", target_id, "transfer", Some(before), Some(after)).await?;
    tracing::info!("[組織管理] 対象=owner_transfer id={target_id:?} 操作=transfer 結果=成功 | admin_id={admin_id}");
    Ok(())
}

/// POST /api/v1/admin/owner-transfer — 顧客または案件と、その配下の担当を付け替える
pub async fn transfer(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Json(req): Json<TransferReq>) -> ApiResult<Json<Value>> {
    let root = parse_root(req.client_id, req.project_id.as_deref()).map_err(bad_input)?;
    let mut tx = begin(&pool).await?;
    let to = validate_target(&mut tx, &req.to).await?;
    let previous = repo::root_stamp(&mut *tx, &root).await?.ok_or(ApiErr::NotFound(match root {
        TransferRoot::Client(_) => "顧客が見つかりません",
        TransferRoot::Project(_) => "案件が見つかりません",
    }))?;
    let updated = repo::transfer_subtree(&mut tx, &root, &to).await?;
    let (kind, key, target_id) = match &root {
        TransferRoot::Client(id) => ("client", json!(id), Some(*id)),
        TransferRoot::Project(pid) => ("project", json!(pid), None),
    };
    let before = json!({ "root": { "kind": kind, "id": key }, "previous_stamp": stamp_json(&previous) });
    let after = json!({ "to": stamp_json(&to), "updated": js(&updated), "total": total(&updated) });
    log_transfer(&mut tx, auth.user.id, target_id, before, after).await?;
    commit(tx).await?;
    Ok(Json(summary(&updated)))
}

/// GET /api/v1/admin/owner-transfer/by-employee/{employee_id} — 社員が担当のデータの件数
pub async fn owned_by_employee(State(pool): State<PgPool>, Path(employee_id): Path<i64>) -> ApiResult<Json<Value>> {
    let mut conn = pool.acquire().await?;
    if repo::employee_is_active(&mut *conn, employee_id).await?.is_none() {
        return Err(ApiErr::NotFound("社員が見つかりません"));
    }
    let owned = repo::count_owned_by_employee(&mut conn, employee_id).await?;
    // 組織ごとの内訳(引き継ぐと、すべて選んだ組織に移ることを確認してもらうため。PR 6b のレビュー指摘)
    let by_org = repo::count_owned_by_org_for_employee(&mut conn, employee_id).await?;
    Ok(Json(json!({ "owned": owned, "by_org": by_org, "total": total(&owned) })))
}

/// POST /api/v1/admin/owner-transfer/by-employee — 社員が担当のデータすべてを付け替える
pub async fn by_employee(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Json(req): Json<ByEmployeeReq>) -> ApiResult<Json<Value>> {
    let mut tx = begin(&pool).await?;
    if repo::employee_is_active(&mut *tx, req.from_employee_id).await?.is_none() {
        return Err(ApiErr::NotFound("引き継ぎ元の社員が見つかりません"));
    }
    let to = validate_target(&mut tx, &req.to).await?;
    check_not_same_employee(req.from_employee_id, &to).map_err(bad_input)?;
    let updated = repo::transfer_by_employee(&mut tx, req.from_employee_id, &to).await?;
    let before = json!({ "root": { "kind": "employee", "id": req.from_employee_id }, "reason": "manual" });
    let after = json!({ "to": stamp_json(&to), "updated": js(&updated), "total": total(&updated) });
    log_transfer(&mut tx, auth.user.id, Some(req.from_employee_id), before, after).await?;
    commit(tx).await?;
    Ok(Json(summary(&updated)))
}

/// 社員(またはその社員に結びついたユーザー)を無効にする前の引き継ぎ(D15)。無効化と同じトランザクションの中で呼ぶ。
///
/// 担当データが無ければ何もせず `None`。あるのに `transfer_to` が無ければ409(`ApiErr::OwnedData`)。
/// あれば検証して付け替え、履歴を1行残し、更新件数を返す。
pub(crate) async fn hand_over(
    conn: &mut PgConnection,
    admin_id: i64,
    employee_id: i64,
    transfer_to: Option<&ToReq>,
    reason: Value,
) -> ApiResult<Option<Vec<TableCount>>> {
    let owned = repo::count_owned_by_employee(&mut *conn, employee_id).await?;
    if total(&owned) == 0 {
        return Ok(None);
    }
    let Some(to_req) = transfer_to else {
        return Err(ApiErr::OwnedData(owned));
    };
    let to = validate_target(conn, to_req).await?;
    check_not_same_employee(employee_id, &to).map_err(bad_input)?;
    let updated = repo::transfer_by_employee(conn, employee_id, &to).await?;
    let before = json!({ "root": { "kind": "employee", "id": employee_id }, "reason": reason });
    let after = json!({ "to": stamp_json(&to), "updated": js(&updated), "total": total(&updated) });
    log_transfer(conn, admin_id, Some(employee_id), before, after).await?;
    Ok(Some(updated))
}

/// 社員を無効にする(論理削除)。担当データがあれば、同じトランザクションで先に引き継ぐ(D15)。
/// 担当データがあるのに引き継ぎ先が無ければ409(何も変わらない)。担当データが無ければ、従来どおり無効にするだけ。
pub async fn deactivate_employee(pool: &PgPool, admin_id: i64, employee_id: i64, transfer_to: Option<&ToReq>) -> ApiResult<Option<Vec<TableCount>>> {
    let mut tx = begin(pool).await?;
    let moved = hand_over(&mut tx, admin_id, employee_id, transfer_to, json!("employee deactivated")).await?;
    employee_repo::deactivate(&mut *tx, employee_id).await?;
    commit(tx).await?;
    Ok(moved)
}

/// `toggle_user` の結果: (切替後の有効状態, 外した割り当て, 引き継いだ件数)。
pub type ToggleOutcome = (bool, Vec<AssignmentRecord>, Option<Vec<TableCount>>);

/// ユーザーの有効/無効を切り替える。無効にするとき、結びついた社員が担当のデータがあれば、同じトランザクションで先に引き継ぐ(D15)。
/// 引き継ぎ先が無ければ409(何も変わらない)。再有効化・社員が結びついていない・担当データが無い場合は、従来どおり。
/// ユーザーがいなければ `None`。
pub async fn toggle_user(pool: &PgPool, admin_id: i64, user_id: i64, transfer_to: Option<&ToReq>) -> ApiResult<Option<ToggleOutcome>> {
    let mut tx = begin(pool).await?;
    let Some((currently_active, employee_id)) = user_repo::active_and_employee(&mut tx, user_id).await? else {
        return Ok(None);
    };
    // 自分自身・最後の有効な管理者は無効にできない(引き継ぎの 409 より先に判定する。DEMO-000186)
    if let Err(refusal) = user_repo::check_can_deactivate(&mut tx, admin_id, user_id).await? {
        return Err(ApiErr::BadRequest(refusal.message().to_string()));
    }
    let mut moved = None;
    if let (true, Some(emp)) = (currently_active, employee_id) {
        let reason = json!({ "text": "user deactivated", "user_id": user_id });
        moved = hand_over(&mut tx, admin_id, emp, transfer_to, reason).await?;
    }
    let Some((is_active, removed)) = user_repo::toggle_active_in_tx(&mut tx, user_id, admin_id).await? else {
        return Ok(None);
    };
    commit(tx).await?;
    Ok(Some((is_active, removed, moved)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_body_means_no_transfer() {
        assert!(matches!(parse_deactivate_body(b""), Ok(None)));
        assert!(matches!(parse_deactivate_body(b" \n"), Ok(None)));
        assert!(matches!(parse_deactivate_body(b"{}"), Ok(None)));
    }

    #[test]
    fn body_with_transfer_to_is_parsed() {
        let t = parse_deactivate_body(br#"{"transfer_to":{"org_unit_id":3,"employee_id":4}}"#).ok().flatten();
        assert_eq!(t.map(|t| (t.org_unit_id, t.employee_id)), Some((Some(3), Some(4))));
    }

    #[test]
    fn broken_body_is_rejected() {
        assert!(matches!(parse_deactivate_body(b"{not json"), Err(ApiErr::BadRequest(_))));
    }
}
