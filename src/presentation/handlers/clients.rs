/// presentation/handlers/clients.rs — クライアント管理ハンドラ
///
/// Askama テンプレート化済み。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Form, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, PgPool};

use super::org_admin::{begin, commit, ApiErr, ApiResult};
use crate::domain::authz::stamp::OwnerStamp;
use crate::infrastructure::repositories::client_repo::{self, ClientFormInput, NewClient};
use crate::infrastructure::repositories::{authz_repo, org_repo, owner_transfer_repo};

// ── ハンドラ ──

/// 取引先登録APIの本文。担当(組織・担当者)は必須。
#[derive(Debug, Deserialize)]
pub struct CreateClientRequest {
    pub name: String,
    #[serde(default)]
    pub contact_person: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    /// 請求単位(`PROJECT` / `CLIENT`)。省略時は案件ごと。
    #[serde(default)]
    pub billing_unit: Option<String>,
    /// 担当組織の ID(必須。有効な組織で、上位も有効なもの)
    #[serde(default)]
    pub owner_org_unit_id: Option<i64>,
    /// 担当者(社員)の ID(必須。有効な社員)
    #[serde(default)]
    pub owner_employee_id: Option<i64>,
}

/// 請求単位の検証。省略は案件ごと。
fn parse_billing_unit(v: Option<&str>) -> ApiResult<&'static str> {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        None | Some("PROJECT") => Ok("PROJECT"),
        Some("CLIENT") => Ok("CLIENT"),
        Some(_) => Err(ApiErr::BadRequest("請求単位は「案件ごと」か「取引先まとめ」を選んでください".to_string())),
    }
}

/// 担当(組織・担当者)の検証。組織は存在し有効で上位も有効、担当者は存在し有効。通ればスタンプにする。
async fn validate_owner(conn: &mut PgConnection, org: Option<i64>, emp: Option<i64>) -> ApiResult<OwnerStamp> {
    let org = org.ok_or_else(|| ApiErr::BadRequest("担当の組織を選んでください".to_string()))?;
    let emp = emp.ok_or_else(|| ApiErr::BadRequest("担当者を選んでください".to_string()))?;
    let units = org_repo::list_all(&mut *conn).await?;
    let unit = units.iter().find(|u| u.id == org).ok_or_else(|| ApiErr::BadRequest("選んだ担当の組織が見つかりません".to_string()))?;
    if !unit.is_active || authz_repo::disabled_org_ids(&units).contains(&org) {
        return Err(ApiErr::BadRequest("無効な組織は担当にできません".to_string()));
    }
    match owner_transfer_repo::employee_is_active(&mut *conn, emp).await? {
        None => Err(ApiErr::BadRequest("選んだ担当者が見つかりません".to_string())),
        Some(false) => Err(ApiErr::BadRequest("無効な担当者は担当にできません".to_string())),
        Some(true) => Ok(OwnerStamp::new(org, emp)),
    }
}

/// POST /api/v1/clients — 取引先(顧客)を登録する。Admin専用。
///
/// 担当(組織と担当者)が必須。この取引先の案件・契約・受注・請求書などは、ここで選んだ担当を写して作られる(設計書 §6.6)。
/// 検証と登録は、組織管理の操作と直列化した1つのトランザクションで行う(検証後に組織が無効になるのを防ぐ)。
pub async fn api_create(State(pool): State<PgPool>, Json(req): Json<CreateClientRequest>) -> ApiResult<(StatusCode, Json<Value>)> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(ApiErr::BadRequest("名称は必須です".to_string()));
    }
    let billing_unit = parse_billing_unit(req.billing_unit.as_deref())?;
    let mut tx = begin(&pool).await?;
    let stamp = validate_owner(&mut tx, req.owner_org_unit_id, req.owner_employee_id).await?;
    let form = NewClient {
        name,
        contact_person: req.contact_person.as_deref().unwrap_or("").trim(),
        email: req.email.as_deref().unwrap_or("").trim(),
        phone: req.phone.as_deref().unwrap_or("").trim(),
        address: req.address.as_deref().unwrap_or("").trim(),
        billing_unit,
    };
    let id = client_repo::create(&mut tx, &form, &stamp).await?;
    commit(tx).await?;
    Ok((StatusCode::CREATED, Json(json!({ "ok": true, "id": id }))))
}

/// 更新処理（POST /clients/:id）
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<ClientForm>,
) -> impl IntoResponse {
    if let Err(e) = client_repo::update(&pool, id, ClientFormInput {
        name: &form.name,
        contact_person: &form.contact_person,
        email: &form.email,
        phone: &form.phone,
        address: &form.address,
        edi_system_type: &form.edi_system_type,
        edi_notification_email: &form.edi_notification_email,
        work_report_email: &form.work_report_email,
        invoice_email: &form.invoice_email,
    }).await { tracing::error!("DB error: {:?}", e); }

    Redirect::to("/clients")
}

#[derive(serde::Deserialize)]
pub struct ClientForm {
    name: String,
    contact_person: String,
    email: String,
    phone: String,
    address: String,
    #[serde(default)]
    edi_system_type: String,
    #[serde(default)]
    edi_notification_email: String,
    #[serde(default)]
    work_report_email: String,
    #[serde(default)]
    invoice_email: String,
}
