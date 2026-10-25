/// domain/models/system.rs — システム系エンティティ
///
/// EmailTemplate（メールテンプレート）, User（認証）, Session, UserProfile

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// メールテンプレート
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct EmailTemplate {
    pub id: i64,
    pub code: String,
    pub subject: String,
    pub body: String,
    pub description: String,
    pub updated_at: DateTime<Utc>,
}

/// 認証ユーザー
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub password: String,
    pub username: String,
    pub is_active: bool,
    pub is_staff: bool,
    pub mfa_enabled: bool,
    /// 全社員の給与データを閲覧・確認・振込済み操作できるか（is_staffとは独立した権限）。
    /// 管理者(is_staff)であることは、この権限を持つための必要条件ではない。
    pub can_view_all_payroll: bool,
    /// 全社員の経費申請を閲覧・承認・差戻しできるか（is_staffとは独立した権限）
    pub can_view_all_expenses: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// セッション
#[derive(Debug, Clone, FromRow)]
pub struct Session {
    pub session_id: String,
    pub user_id: i64,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// ユーザープロフィール
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UserProfile {
    pub id: i64,
    pub user_id: i64,
    pub partner_id: Option<String>,
    pub employee_id: Option<i64>,
    pub is_first_login: bool,
}

/// ログインフォーム
#[derive(Debug, Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}
