/// infrastructure/repositories/company_info_repo.rs — 自社情報(s_company_info)設定管理
///
/// s_company_infoは常に1行のみ（migrations/027_dedupe_company_info_singleton.sqlでユニークインデックス強制）。
/// email_host_password は機密情報のためAPIレスポンスには含めず、設定済みかどうかの真偽値のみ返す。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::value_objects::account_number::AccountNumber;
use crate::infrastructure::repositories::bank_account_repo;

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
    /// 振込先の口座番号(復号済み)。`get_company_info` だけが入れる。`get_company_info_basic` では空のまま
    #[sqlx(skip)]
    pub account_number: AccountNumber,
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
    pub target_margin_direct: i32,
    pub target_margin_subcontract: i32,
    /// 「取引先のメール」として扱わないドメイン(1行に1つ)
    pub excluded_mail_domains: String,
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
    // 印影・ロゴの画像パスは PDF で使っていないため、画面から外した(PDF の印影は scripts/pdf/company_seal.png。DEMO-000201)。
    // 送られてきたときだけ更新し、無ければ既存値を保持する(古い画面とも両立)
    #[serde(default)]
    pub stamp_image: Option<String>,
    #[serde(default)]
    pub logo_image: Option<String>,
    // メールアカウント(SMTP)は「メール > 送信 > 設定」の専用API(update_mail_account)で変える(DEMO-000196)。
    // 自社情報の保存では、送られてきたときだけ更新し、無ければ既存値を保持する(古い画面からの更新とも両立)
    #[serde(default)]
    pub email_host: Option<String>,
    #[serde(default)]
    pub email_port: Option<i32>,
    #[serde(default)]
    pub email_use_tls: Option<bool>,
    #[serde(default)]
    pub email_host_user: Option<String>,
    /// 空文字またはNoneの場合は既存のパスワードを変更しない
    #[serde(default)]
    pub email_host_password: Option<String>,
    #[serde(default)]
    pub default_from_email: Option<String>,
    pub notice_approval_threshold: Option<i32>,
    pub token_expiry_days: Option<i32>,
    /// 利益の目安(直受け %)。未指定なら既存値を保持(古い画面からの更新で失敗させない)
    #[serde(default)]
    pub target_margin_direct: Option<i32>,
    /// 利益の目安(下請け %)。未指定なら既存値を保持
    #[serde(default)]
    pub target_margin_subcontract: Option<i32>,
    /// 除外するメールドメイン。未指定なら既存値を保持(古い画面からの更新で消さない)
    #[serde(default)]
    pub excluded_mail_domains: Option<String>,
}

/// 自社情報を取得する（口座番号を復号して入れる。画面の編集用。パスワード実値は含めない）
pub async fn get_company_info(pool: &PgPool) -> Result<Option<CompanyInfo>> {
    let Some(mut info) = get_company_info_basic(pool).await? else {
        return Ok(None);
    };
    info.account_number = bank_account_repo::read_company_account(pool).await?.unwrap_or_default();
    Ok(Some(info))
}

/// 自社情報を口座番号を読まずに取得する（口座番号以外だけを使う箇所用。account_number は空のまま）
pub async fn get_company_info_basic(pool: &PgPool) -> Result<Option<CompanyInfo>> {
    let row = sqlx::query_as::<_, CompanyInfo>(
        r#"SELECT id, name, postal_code, address, tel, fax, representative_title, representative_name,
                  registration_no, responsible_person, contact_person, bank_name, bank_branch,
                  account_type, account_name, stamp_image, logo_image,
                  email_host, email_port, email_use_tls, email_host_user, default_from_email,
                  notice_approval_threshold, token_expiry_days, target_margin_direct, target_margin_subcontract, excluded_mail_domains,
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

/// 自社情報を更新する（email_host_passwordは値が指定された場合のみ更新。
/// 利益の目安は値が指定されなければ既存値を保持）
pub async fn update_company_info(pool: &PgPool, id: i64, form: &CompanyInfoUpdate) -> Result<()> {
    // 口座番号は平文として作れる値だけ受け付ける(暗号文の接頭辞は拒否)。保存は暗号化経由(bank_account_repo)
    // 入力検証(数字のみ・桁数・全角の直し)を通し、正規化した値だけを書く
    let account = AccountNumber::parse_input(&form.account_number).map_err(|e| anyhow::anyhow!(e.message()))?;
    let mut tx = pool.begin().await?;
    sqlx::query(
        r#"UPDATE s_company_info SET
            name=$1, postal_code=$2, address=$3, tel=$4, fax=$5,
            representative_title=$6, representative_name=$7, registration_no=$8,
            responsible_person=$9, contact_person=$10,
            bank_name=$11, bank_branch=$12, account_type=$13, account_name=$14,
            stamp_image=COALESCE($15, stamp_image), logo_image=COALESCE($16, logo_image),
            email_host=COALESCE($17, email_host), email_port=COALESCE($18, email_port), email_use_tls=COALESCE($19, email_use_tls),
            email_host_user=COALESCE($20, email_host_user), default_from_email=COALESCE($21, default_from_email),
            notice_approval_threshold=$22, token_expiry_days=$23,
            target_margin_direct=COALESCE($25, target_margin_direct), target_margin_subcontract=COALESCE($26, target_margin_subcontract),
            excluded_mail_domains=COALESCE($27, excluded_mail_domains)
           WHERE id=$24"#,
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
    .bind(&form.account_name)
    .bind(form.stamp_image.as_deref())
    .bind(form.logo_image.as_deref())
    .bind(form.email_host.as_deref())
    .bind(form.email_port)
    .bind(form.email_use_tls)
    .bind(form.email_host_user.as_deref())
    .bind(form.default_from_email.as_deref())
    .bind(form.notice_approval_threshold)
    .bind(form.token_expiry_days)
    .bind(id)
    .bind(form.target_margin_direct)
    .bind(form.target_margin_subcontract)
    .bind(form.excluded_mail_domains.as_deref())
    .execute(&mut *tx)
    .await?;

    bank_account_repo::write_account_tx(&mut tx, bank_account_repo::Target::CompanyInfo, &id.to_string(), &account).await?;
    tx.commit().await?;

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

/// 利益の目安を検証する（0〜100の整数）
pub fn validate_margin_target(value: i32) -> Result<()> {
    if value < 0 || value > 100 {
        anyhow::bail!("利益の目安は0〜100の整数である必要があります");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_margin_target_accepts_lower_bound() {
        assert!(validate_margin_target(0).is_ok());
    }

    #[test]
    fn validate_margin_target_accepts_upper_bound() {
        assert!(validate_margin_target(100).is_ok());
    }

    #[test]
    fn validate_margin_target_accepts_middle_value() {
        assert!(validate_margin_target(50).is_ok());
    }

    #[test]
    fn validate_margin_target_rejects_below_lower_bound() {
        assert!(validate_margin_target(-1).is_err());
    }

    #[test]
    fn validate_margin_target_rejects_above_upper_bound() {
        assert!(validate_margin_target(101).is_err());
    }
}

/// 「取引先のメール」として扱わないドメイン(小文字・重複なし)。未設定なら空
pub async fn get_excluded_mail_domains(pool: &PgPool) -> Result<Vec<String>> {
    let raw: Option<String> = sqlx::query_scalar("SELECT excluded_mail_domains FROM s_company_info ORDER BY id LIMIT 1")
        .fetch_optional(pool)
        .await?;
    Ok(crate::domain::services::mail_domains::parse_excluded_domains(&raw.unwrap_or_default()))
}

/// メールアカウント(送信と受信の取り込みで共通。DEMO-000196)。パスワードの実値は含めない
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct MailAccount {
    pub email_host: String,
    pub email_port: Option<i32>,
    pub email_use_tls: bool,
    pub email_host_user: String,
    pub default_from_email: String,
    pub has_smtp_password: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct MailAccountUpdate {
    pub email_host: String,
    #[serde(default)]
    pub email_port: Option<i32>,
    pub email_use_tls: bool,
    pub email_host_user: String,
    /// 空文字またはNoneの場合は既存のパスワードを変更しない
    #[serde(default)]
    pub email_host_password: Option<String>,
    pub default_from_email: String,
}

impl MailAccountUpdate {
    /// パスワードを変えるか(空でない値が送られたときだけ)
    pub fn changes_password(&self) -> bool {
        self.email_host_password.as_deref().is_some_and(|p| !p.is_empty())
    }
}

/// メールアカウントを取得する(自社情報が無ければ None)
pub async fn get_mail_account(pool: &PgPool) -> Result<Option<MailAccount>> {
    let row = sqlx::query_as::<_, MailAccount>(
        "SELECT email_host, email_port, email_use_tls, email_host_user, default_from_email,
                (email_host_password <> '') AS has_smtp_password
         FROM s_company_info ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// メールアカウントを更新する(パスワードは値が指定されたときだけ)。更新した行数を返す(0 = 自社情報が無い)
pub async fn update_mail_account(pool: &PgPool, form: &MailAccountUpdate) -> Result<u64> {
    let mut tx = pool.begin().await?;
    let n = sqlx::query(
        "UPDATE s_company_info SET email_host = $1, email_port = $2, email_use_tls = $3,
                email_host_user = $4, default_from_email = $5",
    )
    .bind(&form.email_host)
    .bind(form.email_port)
    .bind(form.email_use_tls)
    .bind(&form.email_host_user)
    .bind(&form.default_from_email)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if form.changes_password() {
        sqlx::query("UPDATE s_company_info SET email_host_password = $1")
            .bind(form.email_host_password.as_deref())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(n)
}

/// 除外するメールドメインの設定値(入力どおりの文字列。未設定なら空)
pub async fn get_excluded_mail_domains_raw(pool: &PgPool) -> Result<String> {
    let raw: Option<String> = sqlx::query_scalar("SELECT excluded_mail_domains FROM s_company_info ORDER BY id LIMIT 1")
        .fetch_optional(pool)
        .await?;
    Ok(raw.unwrap_or_default())
}

/// 除外するメールドメインを保存する。更新した行数を返す(0 = 自社情報が無い)
pub async fn set_excluded_mail_domains(pool: &PgPool, raw: &str) -> Result<u64> {
    let n = sqlx::query("UPDATE s_company_info SET excluded_mail_domains = $1")
        .bind(raw)
        .execute(pool)
        .await?
        .rows_affected();
    Ok(n)
}
