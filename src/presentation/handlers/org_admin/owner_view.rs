/// 顧客・案件の「今の担当」を読むAPI(担当の変更画面の表示用)。Admin専用。DEMO-000173 PR 6c。
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use super::{ApiErr, ApiResult};
use crate::infrastructure::repositories::owner_transfer_repo;

/// `client_id` か `project_id` のどちらか一方。
#[derive(Debug, Deserialize)]
pub struct OwnerQuery {
    pub client_id: Option<i64>,
    pub project_id: Option<String>,
}

/// GET /api/v1/admin/owner-transfer/current?client_id= | project_id= — 今の担当(組織名・担当者名つき。未設定なら null)
pub async fn current(State(pool): State<PgPool>, Query(q): Query<OwnerQuery>) -> ApiResult<Json<Value>> {
    let row = match (q.client_id, q.project_id.as_deref()) {
        (Some(id), None) => owner_transfer_repo::owner_view_of_client(&pool, id).await?,
        (None, Some(pid)) if !pid.is_empty() => owner_transfer_repo::owner_view_of_project(&pool, pid).await?,
        _ => return Err(ApiErr::BadRequest("client_id か project_id のどちらか一方を指定してください".to_string())),
    };
    let Some(v) = row else {
        return Err(ApiErr::NotFound("顧客または案件が見つかりません"));
    };
    Ok(Json(json!(v)))
}
