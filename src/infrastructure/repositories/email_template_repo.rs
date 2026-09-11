/// infrastructure/repositories/email_template_repo.rs — メールテンプレート・設定リポジトリ

use anyhow::Result;
use sqlx::PgPool;

/// メールテンプレート行
#[derive(Debug, sqlx::FromRow, Clone)]
pub struct EmailTemplateRow {
    pub id: i64,
    pub code: String,
    pub subject: String,
    pub body: String,
}

/// テンプレートをコードから取得
pub async fn find_template_by_code(pool: &PgPool, code: &str) -> Result<Option<EmailTemplateRow>> {
    let row = sqlx::query_as::<_, EmailTemplateRow>(
        "SELECT * FROM s_email_template WHERE code = $1"
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 全テンプレート取得（email_test 用）
pub async fn list_all_templates(pool: &PgPool) -> Result<Vec<EmailTemplateRow>> {
    let rows = sqlx::query_as::<_, EmailTemplateRow>(
        "SELECT * FROM s_email_template ORDER BY code"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// メール送信ログ記録
pub async fn insert_sent_email_log(
    pool: &PgPool,
    recipient: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO h_sent_email (recipient, subject, body) VALUES ($1, $2, $3)"
    )
    .bind(recipient)
    .bind(subject)
    .bind(body)
    .execute(pool)
    .await?;
    Ok(())
}

/// CompanyInfo から company_name, tel を取得
pub async fn get_company_info_name_tel(pool: &PgPool) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT COALESCE(name, ''), COALESCE(tel, '') FROM s_company_info ORDER BY id LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// CompanyInfo から SMTP フィールド値を取得（ホワイトリスト match）
pub async fn get_smtp_column(pool: &PgPool, db_field: &str) -> Result<Option<String>> {
    // ホワイトリスト: 許可されたカラム名のみ
    let allowed_fields = &[
        "email_host",
        "email_port",
        "email_host_user",
        "email_host_password",
        "default_from_email",
    ];

    if !allowed_fields.contains(&db_field) {
        return Err(anyhow::anyhow!("不正なカラム名: {}", db_field));
    }

    // 動的 SQL は build しない（format! 禁止）。各カラムに対して静的 query を用意する
    let row: Option<String> = match db_field {
        "email_host" => {
            sqlx::query_scalar("SELECT email_host FROM s_company_info ORDER BY id LIMIT 1")
                .fetch_optional(pool)
                .await?
        }
        "email_port" => {
            sqlx::query_scalar("SELECT email_port FROM s_company_info ORDER BY id LIMIT 1")
                .fetch_optional(pool)
                .await?
        }
        "email_host_user" => {
            sqlx::query_scalar("SELECT email_host_user FROM s_company_info ORDER BY id LIMIT 1")
                .fetch_optional(pool)
                .await?
        }
        "email_host_password" => {
            sqlx::query_scalar("SELECT email_host_password FROM s_company_info ORDER BY id LIMIT 1")
                .fetch_optional(pool)
                .await?
        }
        "default_from_email" => {
            sqlx::query_scalar("SELECT default_from_email FROM s_company_info ORDER BY id LIMIT 1")
                .fetch_optional(pool)
                .await?
        }
        _ => return Err(anyhow::anyhow!("不正なカラム名: {}", db_field)),
    };
    Ok(row)
}

/// default_from_email を取得
pub async fn get_default_from_email(pool: &PgPool) -> Result<Option<String>> {
    let email = sqlx::query_scalar("SELECT default_from_email FROM s_company_info ORDER BY id LIMIT 1")
        .fetch_optional(pool)
        .await?;
    Ok(email)
}
