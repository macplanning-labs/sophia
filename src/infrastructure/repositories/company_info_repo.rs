/// infrastructure/repositories/company_info_repo.rs — 自社情報(s_company_info)設定管理
///
/// s_company_infoは常に1行のみ（migrations/027_dedupe_company_info_singleton.sqlでユニークインデックス強制）。
/// email_host_password は機密情報のためAPIレスポンスには含めず、設定済みかどうかの真偽値のみ返す。

use anyhow::Result;
use sqlx::PgPool;

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CompanyInfo {
    pub id: i64,
    pub name: String,
    pub postal_code: String,
    pub address: String,
    pub tel: String,
    pub fax: String,
    pub representative_title: String,
    pub representative_name: String,
    pub registration_no: String,
    pub responsible_person: String,
    pub contact_person: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
    pub stamp_image: String,
    pub logo_image: String,
    pub email_host: String,
    pub email_port: Option<i32>,
    pub email_use_tls: bool,
    pub email_host_user: String,
    pub default_from_email: String,
    pub notice_approval_threshold: Option<i32>,
    pub token_expiry_days: Option<i32>,
    pub has_smtp_password: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct CompanyInfoUpdate {
    pub name: String,
    pub postal_code: String,
    pub address: String,
    pub tel: String,
    pub fax: String,
    pub representative_title: String,
    pub representative_name: String,
    pub registration_no: String,
    pub responsible_person: String,
    pub contact_person: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
    pub stamp_image: String,
    pub logo_image: String,
    pub email_host: String,
    pub email_port: Option<i32>,
    pub email_use_tls: bool,
    pub email_host_user: String,
    /// 空文字またはNoneの場合は既存のパスワードを変更しない
    #[serde(default)]
    pub email_host_password: Option<String>,
    pub default_from_email: String,
    pub notice_approval_threshold: Option<i32>,
    pub token_expiry_days: Option<i32>,
}

/// 自社情報を取得する（1行のみの前提。パスワード実値は含めない）
pub async fn get_company_info(pool: &PgPool) -> Result<Option<CompanyInfo>> {
    let row = sqlx::query_as::<_, CompanyInfo>(
        r#"SELECT id, name, postal_code, address, tel, fax, representative_title, representative_name,
                  registration_no, responsible_person, contact_person, bank_name, bank_branch,
                  account_type, account_number, account_name, stamp_image, logo_image,
                  email_host, email_port, email_use_tls, email_host_user, default_from_email,
                  notice_approval_threshold, token_expiry_days,
                  (email_host_password <> '') AS has_smtp_password
           FROM s_company_info ORDER BY id LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// SMTP認証情報の実値を取得する（内部用。IMAP接続にも同じGmailアカウントの認証情報を
/// 流用するため。APIレスポンスには絶対に含めないこと）
pub async fn get_smtp_credentials(pool: &PgPool) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT email_host_user, email_host_password FROM s_company_info
         WHERE email_host_user <> '' AND email_host_password <> '' ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 自社情報を更新する（email_host_passwordは値が指定された場合のみ更新）
pub async fn update_company_info(pool: &PgPool, id: i64, form: &CompanyInfoUpdate) -> Result<()> {
    sqlx::query(
        r#"UPDATE s_company_info SET
            name=$1, postal_code=$2, address=$3, tel=$4, fax=$5,
            representative_title=$6, representative_name=$7, registration_no=$8,
            responsible_person=$9, contact_person=$10,
            bank_name=$11, bank_branch=$12, account_type=$13, account_number=$14, account_name=$15,
            stamp_image=$16, logo_image=$17,
            email_host=$18, email_port=$19, email_use_tls=$20, email_host_user=$21, default_from_email=$22,
            notice_approval_threshold=$23, token_expiry_days=$24
           WHERE id=$25"#,
    )
    .bind(&form.name)
    .bind(&form.postal_code)
    .bind(&form.address)
    .bind(&form.tel)
    .bind(&form.fax)
    .bind(&form.representative_title)
    .bind(&form.representative_name)
    .bind(&form.registration_no)
    .bind(&form.responsible_person)
    .bind(&form.contact_person)
    .bind(&form.bank_name)
    .bind(&form.bank_branch)
    .bind(&form.account_type)
    .bind(&form.account_number)
    .bind(&form.account_name)
    .bind(&form.stamp_image)
    .bind(&form.logo_image)
    .bind(&form.email_host)
    .bind(form.email_port)
    .bind(form.email_use_tls)
    .bind(&form.email_host_user)
    .bind(&form.default_from_email)
    .bind(form.notice_approval_threshold)
    .bind(form.token_expiry_days)
    .bind(id)
    .execute(pool)
    .await?;

    if let Some(pw) = form.email_host_password.as_deref() {
        if !pw.is_empty() {
            sqlx::query("UPDATE s_company_info SET email_host_password = $1 WHERE id = $2")
                .bind(pw)
                .bind(id)
                .execute(pool)
                .await?;
        }
    }

    Ok(())
}
