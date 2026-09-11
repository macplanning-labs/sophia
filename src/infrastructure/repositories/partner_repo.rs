/// infrastructure/repositories/partner_repo.rs — パートナーCRUD
///
/// list_all, find_by_id, count は base.rs マクロで自動生成。
/// create, next_partner_id は業務固有のためここに残す。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::partner::Partner;

// ── 共通CRUD（マクロ生成）──

impl_list_all!(list_all, Partner, "m_partner", "partner_id DESC");
impl_find_by_str_id!(find_by_id, Partner, "m_partner", "partner_id");
impl_count!(count, "m_partner");

// ── 業務固有 ──

/// パートナー登録フォーム入力
pub struct PartnerFormInput<'a> {
    pub name: &'a str,
    pub email: &'a str,
    pub tel: &'a str,
    pub address: &'a str,
    pub representative_name: &'a str,
}

/// パートナー登録（partner_idは呼び出し元で採番済みのものを渡す）
pub async fn create(pool: &PgPool, partner_id: &str, form: PartnerFormInput<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO m_partner (partner_id, name, email, tel, address, representative_name) VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(partner_id)
    .bind(form.name)
    .bind(form.email)
    .bind(form.tel)
    .bind(form.address)
    .bind(form.representative_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// パートナー更新
pub async fn update(pool: &PgPool, partner_id: &str, form: PartnerFormInput<'_>) -> Result<()> {
    sqlx::query(
        "UPDATE m_partner SET name=$1, email=$2, tel=$3, address=$4, representative_name=$5 WHERE partner_id=$6"
    )
    .bind(form.name)
    .bind(form.email)
    .bind(form.tel)
    .bind(form.address)
    .bind(form.representative_name)
    .bind(partner_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// パートナーの (partner_id, name) 一覧（フィルタ用プルダウン等）
pub async fn list_id_name_options(pool: &PgPool) -> Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT partner_id, name FROM m_partner ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 次のパートナーIDを生成（10桁連番）
pub async fn next_partner_id(pool: &PgPool) -> Result<String> {
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT MAX(partner_id) FROM m_partner"
    )
    .fetch_one(pool)
    .await?;

    let next = match row.0 {
        Some(max_id) => {
            let num: i64 = max_id.parse().unwrap_or(0);
            format!("{:010}", num + 1)
        }
        None => "0000000001".to_string(),
    };

    Ok(next)
}

/// メールテスト用：最新の partner_contract_id を取得
pub async fn find_latest_partner_contract_id(pool: &PgPool) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_partner_contract ORDER BY created_at DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(id)
}
