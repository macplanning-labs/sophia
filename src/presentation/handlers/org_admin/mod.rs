/// presentation/handlers/org_admin — 組織・ロール・割り当ての管理API(Admin専用。DEMO-000173 PR 5)
///
/// - `orgs`: 組織の木(一覧・作成・更新・付け替え・有効/無効)
/// - `roles`: ロールと権限セット、権限カタログ
/// - `assignments`: ユーザーへの割り当て、変更履歴
/// - `transfer`: 担当の引き継ぎ(顧客・案件・社員単位)
///
/// 変更系はすべて、1つのトランザクションの中で「管理操作の直列化ロック → 検証 → 更新 → 履歴(h_org_role_change)」を行う。
/// 検証は `domain::authz`(木・割り当て・権限セット)の関数で行い、ハンドラに規則を書かない。
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

use crate::domain::authz::admin_input::org_tree_error_message;
use crate::domain::authz::org_tree::OrgTree;
use crate::infrastructure::db_tx::commit_checked;
use crate::infrastructure::repositories::org_change_log_repo;
use crate::infrastructure::repositories::owner_transfer_repo::{self, TableCount};
use crate::infrastructure::repositories::org_repo::{self, OrgUnitRow};

pub mod assignments;
pub mod orgs;
pub mod roles;
pub mod owner_view;
pub mod receiver;
pub mod transfer;

/// APIのエラー。本文は `{"error": "日本語のメッセージ"}`。
#[derive(Debug)]
pub enum ApiErr {
    BadRequest(String),
    NotFound(&'static str),
    Conflict(String),
    /// 無効にしようとした社員が担当のデータがあり、引き継ぎ先が指定されていない(409。件数つき)。
    OwnedData(Vec<TableCount>),
    Internal(anyhow::Error),
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        let (status, msg) = match self {
            ApiErr::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiErr::NotFound(m) => (StatusCode::NOT_FOUND, m.to_string()),
            ApiErr::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiErr::OwnedData(owned) => {
                let body = json!({
                    "error": "この社員が担当のデータがあります。引き継ぎ先(組織・担当者)を選んでください",
                    "total": owner_transfer_repo::total(&owned),
                    "owned": owned,
                });
                return (StatusCode::CONFLICT, Json(body)).into_response();
            }
            ApiErr::Internal(e) => {
                tracing::error!("[組織管理] 結果=失敗 影響=操作できない | error={e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "内部エラーが発生しました".to_string())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<anyhow::Error> for ApiErr {
    fn from(e: anyhow::Error) -> Self {
        // 一意制約・外部キー制約の違反は、利用者に分かる409にする。
        if let Some(sqlx::Error::Database(db)) = e.downcast_ref::<sqlx::Error>() {
            match (db.code().as_deref(), db.constraint()) {
                (Some("23505"), Some("uq_org_unit_sibling_name")) => {
                    return ApiErr::Conflict("同じ親の下に、同じ名前の有効な組織があります".to_string())
                }
                (Some("23505"), Some("m_role_code_key")) => return ApiErr::Conflict("同じコードのロールが既にあります".to_string()),
                (Some("23505"), Some(c)) if c.starts_with("s_user_org_role") => {
                    return ApiErr::Conflict("同じ割り当てが既にあります".to_string())
                }
                (Some("23505"), _) => return ApiErr::Conflict("既に登録されています".to_string()),
                (Some("23503"), _) => return ApiErr::Conflict("他のデータから使われているため、変更できません".to_string()),
                _ => {}
            }
        }
        ApiErr::Internal(e)
    }
}

impl From<sqlx::Error> for ApiErr {
    fn from(e: sqlx::Error) -> Self {
        ApiErr::from(anyhow::Error::from(e))
    }
}

pub(crate) type ApiResult<T> = Result<T, ApiErr>;

/// 管理操作のトランザクションを始める(他の管理操作と直列化する)。
pub(crate) async fn begin(pool: &PgPool) -> ApiResult<Transaction<'static, Postgres>> {
    let mut tx = pool.begin().await?;
    org_change_log_repo::lock_admin_changes(&mut tx).await?;
    Ok(tx)
}

/// トランザクションを確定する(失敗状態なら取り消してエラー)。
pub(crate) async fn commit(tx: Transaction<'static, Postgres>) -> ApiResult<()> {
    commit_checked(tx).await?;
    Ok(())
}

/// 全組織と、その木を読み込む。
pub(crate) async fn load_tree(conn: &mut PgConnection) -> ApiResult<(Vec<OrgUnitRow>, OrgTree)> {
    let units = org_repo::list_all(&mut *conn).await?;
    let pairs: Vec<(i64, Option<i64>)> = units.iter().map(|u| (u.id, u.parent_id)).collect();
    let tree = OrgTree::new(&pairs).map_err(|e| ApiErr::BadRequest(org_tree_error_message(&e)))?;
    Ok((units, tree))
}

/// 値をJSONにする(履歴の before/after 用)。失敗時は null にするが、履歴の欠落に気づけるようエラーログを出す。
pub(crate) fn js<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or_else(|e| {
        tracing::error!("[組織管理] 履歴の内容をJSONにできません(null で記録します) 影響=変更履歴の内容が欠ける | error={e}");
        Value::Null
    })
}

/// 履歴を1行残す。
pub(crate) async fn log_change(
    conn: &mut PgConnection,
    admin_id: i64,
    target_kind: &str,
    target_id: i64,
    action: &str,
    before: Option<Value>,
    after: Option<Value>,
) -> ApiResult<()> {
    org_change_log_repo::insert(conn, admin_id, target_kind, Some(target_id), action, before, after).await?;
    tracing::info!("[組織管理] 対象={target_kind} id={target_id} 操作={action} 結果=成功 | admin_id={admin_id}");
    Ok(())
}
