/// presentation/handlers/clients.rs — クライアント管理ハンドラ
///
/// Askama テンプレート化済み。

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::infrastructure::repositories::client_repo::{self, ClientFormInput};

// ── テンプレート定義 ──




// ── ハンドラ ──

/// 新規作成処理（POST /clients）
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<ClientForm>,
) -> impl IntoResponse {
    if let Err(e) = client_repo::create(&pool, ClientFormInput {
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
