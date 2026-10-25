/// infrastructure/repositories/partner_repo.rs — パートナーCRUD
///
/// list_all, find_by_id は列を明示して読む(全列の指定を使わない)。口座番号は暗号化された列のため、
/// ここでは読まない(画面用の取得は bank_account_repo 経由。DEMO-000233 S2-G2-3)。
/// count は base.rs マクロで自動生成。create, next_partner_id は業務固有のためここに残す。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::partner::Partner;

impl_count!(count, "m_partner");

/// パートナーの読み取り列。`Partner` の項目と同じ並び。暗号化された口座番号の列は含めない
const PARTNER_SELECT: &str = "SELECT partner_id, name, name_kana, postal_code, address, tel, fax, email, \
    report_email, cc, bcc, representative_name, representative_name_kana, representative_position, \
    responsible_person, contact_person, registration_no, staff_contact_id, attachment_file, bank_name, \
    bank_branch, account_type, account_name, peppol_participant_id FROM m_partner";

/// パートナーの全件(partner_id の降順)。口座番号は読まない
pub async fn list_all(pool: &PgPool) -> Result<Vec<Partner>> {
    let sql = format!("{PARTNER_SELECT} ORDER BY partner_id DESC");
    let rows = sqlx::query_as::<_, Partner>(&sql).fetch_all(pool).await?;
    Ok(rows)
}

/// パートナー 1 件(partner_id で指定)。口座番号は読まない
pub async fn find_by_id(pool: &PgPool, id: &str) -> Result<Option<Partner>> {
    let sql = format!("{PARTNER_SELECT} WHERE partner_id = $1");
    let row = sqlx::query_as::<_, Partner>(&sql).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

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
