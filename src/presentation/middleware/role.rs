/// presentation/middleware/role.rs — ロールベースアクセス制御
///
/// 4ロールで制御する:
/// - Admin:     管理者（is_staff=true）→ 全機能 + ユーザー管理 + 設定
/// - Employee:  社員（employee_id あり）→ 業務操作（受発注/稼働/経費/給与閲覧）
/// - Partner:   パートナー（partner_id あり）→ ポータルのみ
/// - Engineer:  パートナー技術者（m_engineer直接認証）→ 日次作業報告のみ
/// - Anonymous: 未認証 → アクセス不可

use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

use crate::domain::models::system::{User, UserProfile};

// ── ロール定義 ──

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    Employee,
    Partner,
    Engineer,
    Anonymous,
}

impl Role {
    /// テンプレート表示用の文字列
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "ADMIN",
            Role::Employee => "EMPLOYEE",
            Role::Partner => "PARTNER",
            Role::Engineer => "ENGINEER",
            Role::Anonymous => "ANONYMOUS",
        }
    }

    /// 日本語表示
    pub fn display(&self) -> &'static str {
        match self {
            Role::Admin => "管理者",
            Role::Employee => "社員",
            Role::Partner => "パートナー",
            Role::Engineer => "エンジニア",
            Role::Anonymous => "未認証",
        }
    }
}

/// ユーザーのロールを判定する
///
/// 優先順位: Admin > Employee > Partner > Anonymous
pub fn get_role(user: &User, profile: Option<&UserProfile>) -> Role {
    if user.is_staff {
        return Role::Admin;
    }
    if let Some(p) = profile {
        if p.employee_id.is_some() {
            return Role::Employee;
        }
        if p.partner_id.is_some() {
            return Role::Partner;
        }
    }
    Role::Anonymous
}

/// User + Profile のペア（Extension に注入する型）
///
/// スタッフ/社員の場合: `from_user()` で構築（s_user + s_user_profile ベース）
/// エンジニアの場合: `from_engineer()` で構築（m_engineer ベース、s_user 不要）
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user: User,
    pub profile: Option<UserProfile>,
    /// エンジニア直接認証の場合に設定される
    direct_engineer_id: Option<i64>,
    direct_partner_id: Option<String>,
}

impl AuthUser {
    /// s_user + s_user_profile からの構築（管理者・社員用）
    pub fn from_user(user: User, profile: Option<UserProfile>) -> Self {
        Self {
            user,
            profile,
            direct_engineer_id: None,
            direct_partner_id: None,
        }
    }

    /// m_engineer からの直接構築（エンジニア用 — パスワード不要マジックリンク認証）
    pub fn from_engineer(engineer_id: i64, name: String, email: String, partner_id: Option<String>) -> Self {
        // ダミーの User を作成（互換性のため）
        let user = User {
            id: engineer_id,
            email,
            password: String::new(),
            username: name,
            is_active: true,
            is_staff: false,
            mfa_enabled: false,
            can_view_all_payroll: false,
            can_view_all_expenses: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        Self {
            user,
            profile: None,
            direct_engineer_id: Some(engineer_id),
            direct_partner_id: partner_id,
        }
    }

    pub fn role(&self) -> Role {
        if self.direct_engineer_id.is_some() {
            return Role::Engineer;
        }
        get_role(&self.user, self.profile.as_ref())
    }

    /// Admin かどうか
    pub fn is_admin(&self) -> bool {
        self.role() == Role::Admin
    }

    /// 社員（Admin/Employee）で MFA 未登録なら登録が必須（パートナー・エンジニアは対象外）
    pub fn requires_mfa_enrollment(&self) -> bool {
        matches!(self.role(), Role::Admin | Role::Employee) && !self.user.mfa_enabled
    }

    /// 全社員の給与データを閲覧・確認・振込済み操作できるか。
    /// is_staff（管理者）とは独立したフラグで判定する（給与閲覧権限と一般管理者権限の分離）。
    /// エンジニア直接認証の場合は常にfalse（給与閲覧の対象外）。
    pub fn can_view_all_payroll(&self) -> bool {
        self.direct_engineer_id.is_none() && self.user.can_view_all_payroll
    }

    /// 全社員の経費申請を閲覧・承認・差戻しできるか。
    /// - 管理者（is_staff）は常に可
    /// - 一般ユーザーは `can_view_all_expenses` フラグで個別付与
    /// - エンジニア直接認証の場合は常に false
    pub fn can_view_all_expenses(&self) -> bool {
        if self.direct_engineer_id.is_some() {
            return false;
        }
        self.user.is_staff || self.user.can_view_all_expenses
    }

    /// パートナーID（Partner ロール用、またはエンジニアの所属パートナー）
    pub fn partner_id(&self) -> Option<&str> {
        if let Some(ref pid) = self.direct_partner_id {
            return Some(pid.as_str());
        }
        self.profile.as_ref()
            .and_then(|p| p.partner_id.as_deref())
    }

    /// 社員ID（Employee ロール用）
    pub fn employee_id(&self) -> Option<i64> {
        self.profile.as_ref()
            .and_then(|p| p.employee_id)
    }

    /// エンジニアID（Engineer ロール用 — m_engineer.id を直接返す）
    pub fn engineer_id(&self) -> Option<i64> {
        self.direct_engineer_id
    }

    /// ロール文字列（テンプレート用）
    pub fn role_str(&self) -> &'static str {
        self.role().as_str()
    }
}

// ── ミドルウェア ──

/// Admin専用ガード — Employee/Partner/Anonymous は 403
///
/// 対象: ユーザー管理, 社員管理, セキュリティ設定
pub async fn admin_required(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let auth_user = request.extensions().get::<AuthUser>();
    match auth_user {
        Some(au) if au.role() == Role::Admin => next.run(request).await,
        Some(_) => StatusCode::FORBIDDEN.into_response(),
        None => Redirect::to("/login").into_response(),
    }
}

/// Employee以上ガード — Admin + Employee がアクセス可能
///
/// 対象: ダッシュボード, 受発注, 稼働報告, 経費申請, 給与閲覧, タスク等
pub async fn employee_or_admin_required(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let auth_user = request.extensions().get::<AuthUser>();
    match auth_user {
        Some(au) if au.role() == Role::Admin || au.role() == Role::Employee => {
            next.run(request).await
        }
        Some(au) if au.role() == Role::Partner || au.role() == Role::Engineer => {
            // パートナー/エンジニアはポータルへリダイレクト
            Redirect::to("/portal/timesheet-entry").into_response()
        }
        Some(_) => StatusCode::FORBIDDEN.into_response(),
        None => Redirect::to("/login").into_response(),
    }
}

/// PARTNER専用ガード — ADMINはスタッフダッシュボードへリダイレクト
pub async fn partner_required(
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let auth_user = request.extensions().get::<AuthUser>();
    match auth_user {
        Some(au) if au.role() == Role::Partner || au.role() == Role::Engineer => {
            next.run(request).await
        }
        Some(au) if au.role() == Role::Admin || au.role() == Role::Employee => {
            // スタッフ・社員はスタッフダッシュボードへ
            Redirect::to("/").into_response()
        }
        Some(_) => StatusCode::FORBIDDEN.into_response(),
        None => {
            if is_api {
                // API: JSON 401を返す（フロントエンドがハンドリング）
                (StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({
                    "error": "認証が必要です",
                    "redirect": "/portal/login"
                }))).into_response()
            } else {
                // ブラウザ: フルURLでリダイレクト（ポート番号保持）
                let base_url = std::env::var("BASE_URL")
                    .unwrap_or_else(|_| String::new());
                let login_url = if base_url.is_empty() {
                    "/portal/login".to_string()
                } else {
                    format!("{}/portal/login", base_url)
                };
                Redirect::to(&login_url).into_response()
            }
        }
    }
}

/// リソースのパートナーIDと認証ユーザーのパートナーIDが一致するか検証する
/// ADMINは常にアクセス可能。
pub fn check_partner_access(auth_user: &AuthUser, resource_partner_id: &str) -> Result<(), StatusCode> {
    match auth_user.role() {
        Role::Admin => Ok(()),
        Role::Partner | Role::Engineer => {
            if auth_user.partner_id() == Some(resource_partner_id) {
                Ok(())
            } else {
                Err(StatusCode::FORBIDDEN)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

// 後方互換: 旧名 staff_required は admin_required のエイリアス
