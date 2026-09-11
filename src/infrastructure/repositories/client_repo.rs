/// infrastructure/repositories/client_repo.rs — クライアントCRUD
///
/// list_all, find_by_id, count は base.rs マクロで自動生成。
/// create, update は業務固有のためここに残す。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::client::Client;

// ── 共通CRUD（マクロ生成）──

impl_list_all!(list_all, Client, "m_client", "id DESC");
impl_find_by_id!(find_by_id, Client, "m_client", "id");
impl_count!(count, "m_client");

// ── 業務固有 ──

/// クライアントの (id, name) 一覧（フィルタ用プルダウン等）
pub async fn list_id_name_options(pool: &PgPool) -> Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM m_client ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// クライアント登録フォーム入力
pub struct ClientFormInput<'a> {
    pub name: &'a str,
    pub contact_person: &'a str,
    pub email: &'a str,
    pub phone: &'a str,
    pub address: &'a str,
    pub edi_system_type: &'a str,
    pub edi_notification_email: &'a str,
    pub work_report_email: &'a str,
    pub invoice_email: &'a str,
}

/// クライアント登録（クライアント管理画面フォームから）
pub async fn create(pool: &PgPool, form: ClientFormInput<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO m_client (name, contact_person, email, phone, address, edi_system_type, edi_notification_email, work_report_email, invoice_email) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(form.name)
    .bind(form.contact_person)
    .bind(form.email)
    .bind(form.phone)
    .bind(form.address)
    .bind(form.edi_system_type)
    .bind(form.edi_notification_email)
    .bind(form.work_report_email)
    .bind(form.invoice_email)
    .execute(pool)
    .await?;
    Ok(())
}

/// クライアント更新（クライアント管理画面フォームから）
pub async fn update(pool: &PgPool, id: i64, form: ClientFormInput<'_>) -> Result<()> {
    sqlx::query(
        "UPDATE m_client SET name=$1, contact_person=$2, email=$3, phone=$4, address=$5, edi_system_type=$6, edi_notification_email=$7, work_report_email=$8, invoice_email=$9 WHERE id=$10"
    )
    .bind(form.name)
    .bind(form.contact_person)
    .bind(form.email)
    .bind(form.phone)
    .bind(form.address)
    .bind(form.edi_system_type)
    .bind(form.edi_notification_email)
    .bind(form.work_report_email)
    .bind(form.invoice_email)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
