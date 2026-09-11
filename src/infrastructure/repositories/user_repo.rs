/// infrastructure/repositories/user_repo.rs — ユーザー管理(s_user/s_user_profile) CRUD

use anyhow::Result;
use sqlx::PgPool;

/// 一覧・詳細共通の表示用行
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct UserRow {
    pub id: i64,
    pub email: String,
    pub username: String,
    pub is_active: bool,
    pub is_staff: bool,
    pub mfa_enabled: bool,
    /// 全社員の給与データを閲覧・確認・振込済み操作できるか（is_staffとは独立した権限）
    pub can_view_all_payroll: bool,
    /// 全社員の経費申請を閲覧・承認・差戻しできるか（is_staffとは独立した権限）
    pub can_view_all_expenses: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub partner_id: Option<String>,
    pub employee_id: Option<i64>,
    pub employee_name: Option<String>,
    pub partner_name: Option<String>,
    pub role_display: Option<String>,
}

/// ユーザーを作成する（作成されたidを返す）
pub async fn insert_user(pool: &PgPool, email: &str, password_hash: &str, username: &str, is_staff: bool) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO s_user (email, password, username, is_staff) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind(email)
    .bind(password_hash)
    .bind(username)
    .bind(is_staff)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// ユーザープロフィールを新規作成する（employee_id / partner_id 紐付け）
pub async fn insert_user_profile(
    pool: &PgPool,
    user_id: i64,
    partner_id: Option<&str>,
    employee_id: Option<i64>,
    is_first_login: bool,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_user_profile (user_id, partner_id, employee_id, is_first_login) VALUES ($1, $2, $3, $4)"
    )
    .bind(user_id)
    .bind(partner_id)
    .bind(employee_id)
    .bind(is_first_login)
    .execute(pool)
    .await?;
    Ok(())
}

/// ユーザープロフィールをupsertする（既存なら partner_id / employee_id を更新）
pub async fn upsert_user_profile(
    pool: &PgPool,
    user_id: i64,
    partner_id: Option<&str>,
    employee_id: Option<i64>,
    is_first_login: bool,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO s_user_profile (user_id, partner_id, employee_id, is_first_login)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (user_id) DO UPDATE SET
            partner_id = EXCLUDED.partner_id,
            employee_id = EXCLUDED.employee_id
        "#
    )
    .bind(user_id)
    .bind(partner_id)
    .bind(employee_id)
    .bind(is_first_login)
    .execute(pool)
    .await?;
    Ok(())
}

/// ユーザー基本情報を更新する
pub async fn update_user_basic(
    pool: &PgPool, id: i64, email: &str, username: &str, is_staff: bool,
    can_view_all_payroll: bool, can_view_all_expenses: bool,
) -> Result<()> {
    sqlx::query(
        "UPDATE s_user SET email = $2, username = $3, is_staff = $4, can_view_all_payroll = $5, can_view_all_expenses = $6, updated_at = NOW() WHERE id = $1"
    )
    .bind(id)
    .bind(email)
    .bind(username)
    .bind(is_staff)
    .bind(can_view_all_payroll)
    .bind(can_view_all_expenses)
    .execute(pool)
    .await?;
    Ok(())
}

/// ユーザーのパスワードを更新する
pub async fn update_password(pool: &PgPool, id: i64, password_hash: &str) -> Result<()> {
    sqlx::query("UPDATE s_user SET password = $2, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .bind(password_hash)
        .execute(pool)
        .await?;
    Ok(())
}

/// ユーザーのアクティブフラグをトグルする（SSR用。結果値は返さない）
pub async fn toggle_active(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query("UPDATE s_user SET is_active = NOT is_active, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ユーザーのアクティブフラグをトグルする（SPA用。トグル後の値を返す）
pub async fn toggle_active_returning(pool: &PgPool, id: i64) -> Result<bool> {
    let is_active: bool = sqlx::query_scalar(
        "UPDATE s_user SET is_active = NOT is_active, updated_at = NOW() WHERE id = $1 RETURNING is_active"
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(is_active)
}

/// ユーザー一覧（社員名・パートナー名JOIN済み）
pub async fn list_user_rows(pool: &PgPool) -> Result<Vec<UserRow>> {
    let rows = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT u.id, u.email, u.username, u.is_active, u.is_staff, u.mfa_enabled,
               u.can_view_all_payroll, u.can_view_all_expenses,
               u.created_at, p.partner_id, p.employee_id,
               COALESCE(e.last_name || e.first_name, '') AS employee_name,
               COALESCE(mp.name, '') AS partner_name,
               CASE WHEN u.is_staff THEN '管理者' ELSE '一般' END AS role_display
        FROM s_user u
        LEFT JOIN s_user_profile p ON p.user_id = u.id
        LEFT JOIN m_employee e ON e.id = p.employee_id
        LEFT JOIN m_partner mp ON mp.partner_id = p.partner_id
        ORDER BY u.id
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// ユーザー詳細（社員名・パートナー名JOIN済み）
pub async fn find_user_detail(pool: &PgPool, id: i64) -> Result<Option<UserRow>> {
    let row = sqlx::query_as::<_, UserRow>(
        r#"
        SELECT u.id, u.email, u.username, u.is_active, u.is_staff, u.mfa_enabled,
               u.can_view_all_payroll, u.can_view_all_expenses, u.created_at,
               p.partner_id, p.employee_id,
               emp.last_name || ' ' || emp.first_name AS employee_name,
               pt.name AS partner_name,
               CASE
                   WHEN u.is_staff THEN '管理者'
                   WHEN p.employee_id IS NOT NULL THEN '社員'
                   WHEN p.partner_id IS NOT NULL THEN 'パートナー'
                   ELSE '未設定'
               END AS role_display
        FROM s_user u
        LEFT JOIN s_user_profile p ON u.id = p.user_id
        LEFT JOIN m_employee emp ON p.employee_id = emp.id
        LEFT JOIN m_partner pt ON p.partner_id = pt.partner_id
        WHERE u.id = $1
        "#
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// メールアドレスの重複件数を取得する
pub async fn count_by_email(pool: &PgPool, email: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_user WHERE email = $1")
        .bind(email)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// メールアドレスの重複件数を取得する（指定id以外）
pub async fn count_by_email_excluding(pool: &PgPool, email: &str, exclude_id: i64) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_user WHERE email = $1 AND id != $2")
        .bind(email)
        .bind(exclude_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// ユーザーを削除する（更新件数を返す）
///
/// s_user_profile / s_session / MFA関連テーブルは ON DELETE CASCADE で自動削除される。
pub async fn delete_user(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM s_user WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
