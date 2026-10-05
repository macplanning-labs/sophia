/// domain/models/system.rs — システム系エンティティ
///
/// CompanyInfo（自社情報）, EmailTemplate（メールテンプレート）, User（認証）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 自社登録情報（PDF・メール等で使用）
#[derive(Debug, Clone, FromRow, Serialize)]
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
    pub tax_rate: rust_decimal::Decimal,
    pub email_host: String,
    pub email_port: Option<i32>,
    pub email_use_tls: bool,
    pub email_host_user: String,
    pub email_host_password: String,
    pub default_from_email: String,
    pub notice_approval_threshold: Option<i32>,
    pub token_expiry_days: Option<i32>,
}

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
