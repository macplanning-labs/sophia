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

use crate::domain::authz::view_all::{can_see_all, can_see_all_payroll_slips, ViewAllInput};
use crate::domain::authz::stamp::Stamped;
use crate::domain::authz::{Authz, Permission};
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
    /// 社内 SSO（Cloudflare Access）でログインしたトークンか。JWT の auth_method から設定する。
    /// SSO は IdP が本人確認（パスキー・2 段階認証）を済ませているため、Sophia の MFA の登録を強制しない
    /// （DEMO-000226 決定 6）。パスワードなど、ほかの方式のトークンは false のまま。
    sso_login: bool,
}

impl AuthUser {
    /// s_user + s_user_profile からの構築（管理者・社員用）
    pub fn from_user(user: User, profile: Option<UserProfile>) -> Self {
        Self {
            user,
            profile,
            direct_engineer_id: None,
            direct_partner_id: None,
            sso_login: false,
        }
    }

    /// 社内 SSO でログインしたトークンから作った利用者として印を付ける。
    pub fn with_sso_login(mut self, sso_login: bool) -> Self {
        self.sso_login = sso_login;
        self
    }

    /// 社内 SSO でログインしたか。
    pub fn is_sso_login(&self) -> bool {
        self.sso_login
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
            sso_login: false,
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
        // SSO でログインしたトークンは免除する(IdP が本人確認を済ませている。決定 6)。
        // 免除は、そのトークンだけ。DB の mfa_enabled は変えないので、パスワードでログインすれば従来どおり求める。
        matches!(self.role(), Role::Admin | Role::Employee)
            && !self.user.mfa_enabled
            && !self.sso_login
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

    /// 「全員分」の判定の入力を作る(DEMO-000173 PR 8)。`legacy_flag` は既存スイッチの値。
    fn view_all_input<'a>(&self, legacy_flag: bool, authz: Option<&'a Authz>) -> ViewAllInput<'a> {
        ViewAllInput {
            is_engineer: self.direct_engineer_id.is_some(),
            is_admin: self.user.is_staff,
            legacy_flag,
            authz,
        }
    }

    /// 全社員の給与明細を見てよいか。`can_view_all_payroll` スイッチ、またはロールの `payroll.slip:view`(全社)。
    /// **管理者であることだけでは見られない**(給与は管理者の権限と分ける。DEMO-000167 項目5)。
    /// `authz` は入口のミドルウェアが読み込んだ権限(無ければ None)。PR 10 でスイッチを移行用ロールに一本化するまで、スイッチも効く。
    pub fn can_see_all_payroll(&self, authz: Option<&Authz>) -> bool {
        can_see_all_payroll_slips(self.view_all_input(self.user.can_view_all_payroll, authz))
    }

    /// 全社員の社員情報を見てよいか。管理者、`can_view_all_payroll` スイッチ(従来どおり)、または
    /// ロールの `hr.employee:view`(全社)のどれか。給与の金額を見てよいかは別(`can_see_all_payroll`)。
    pub fn can_see_all_employees(&self, authz: Option<&Authz>) -> bool {
        can_see_all(self.view_all_input(self.user.can_view_all_payroll, authz), Permission::HrEmployeeView)
    }

    /// 全社員の経費申請を見てよいか。管理者、`can_view_all_expenses` スイッチ、または
    /// ロールの `expense.claim:view`(全社)のどれか。
    /// 注意: 経費の「承認」はここでは決まらない(管理者のみ。`can_approve_expense`)。
    pub fn can_see_all_expenses(&self, authz: Option<&Authz>) -> bool {
        can_see_all(self.view_all_input(self.user.can_view_all_expenses, authz), Permission::ExpenseClaimView)
    }

    /// 担当スタンプの付いたデータ1件を、権限 `perm` で扱ってよいか(ハンドラ自身の判定。入口の権限表に頼らない)。
    /// 管理者は常に可。それ以外は、入口が読み込んだ権限(`authz`)が無ければ不可(読み込み失敗・未読込は拒否側に倒す)。
    /// エンジニア直接認証は常に不可。
    pub fn can_access_stamped(&self, authz: Option<&Authz>, perm: Permission, target: &impl Stamped) -> bool {
        if self.direct_engineer_id.is_some() {
            return false;
        }
        self.is_admin() || authz.is_some_and(|a| a.can(perm, target))
    }

    /// 権限 `perm` で見える範囲(一覧の絞り込み条件)。管理者は全件。エンジニア直接認証と、権限の未読込は、何も見えない
    /// (読み込み失敗・未読込は拒否側に倒す)。ハンドラ自身の判定で、入口の権限表に頼らない。
    pub fn list_filter(&self, authz: Option<&Authz>, perm: Permission) -> crate::domain::authz::list_filter::ListFilter {
        use crate::domain::authz::list_filter::{Clause, ListFilter};
        if self.direct_engineer_id.is_some() {
            return ListFilter::default();
        }
        if self.is_admin() {
            return ListFilter(vec![Clause::All]);
        }
        authz.map(|a| a.list_filter(perm)).unwrap_or_default()
    }

    /// 担当スタンプの無いデータ(添付ファイル等)を、権限 `perm` で扱ってよいか。権限を持っているかだけで決まる。
    pub fn can_access_unstamped(&self, authz: Option<&Authz>, perm: Permission) -> bool {
        if self.direct_engineer_id.is_some() {
            return false;
        }
        self.is_admin() || authz.is_some_and(|a| a.can_unstamped(perm))
    }

    /// 経費の承認・承認取り消しをしてよいか(D12: 決裁は管理者のみ)。
    /// 管理者か、ロールの `expense.claim:approve` を持つ人。`can_view_all_expenses` スイッチだけでは不可(PR 8 で外した)。
    /// 標準ロールで承認を持つのは管理者だけ(移行用の「経費全件」ロールの承認は、移行 SQL で外す)。
    pub fn can_approve_expense(&self, authz: Option<&Authz>) -> bool {
        self.direct_engineer_id.is_none()
            && (self.user.is_staff || authz.is_some_and(|a| a.has_any(Permission::ExpenseClaimApprove)))
    }

    /// 経費の差戻しをしてよいか(D12: 一次チェックは経理・人事にも開放)。
    /// 管理者、ロールの `expense.claim:review` を持つ人、または `can_view_all_expenses` スイッチの保持者。
    /// スイッチの保持者は、移行用ロール「経費全件」(承認・差戻し・閲覧)と同じ扱いだったので、
    /// 移行用ロールの割り当てが済む前に差戻しができなくならないよう、差戻しだけはスイッチも効かせる(PR 10 で整理)。
    pub fn can_review_expense(&self, authz: Option<&Authz>) -> bool {
        self.direct_engineer_id.is_none()
            && (self.user.is_staff
                || self.user.can_view_all_expenses
                || authz.is_some_and(|a| a.has_any(Permission::ExpenseClaimReview)))
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

#[cfg(test)]
mod sso_enrollment_tests {
    //! SSO でログインしたトークンは、Sophia の MFA の登録を強制しない(DEMO-000226 決定 6)。
    //! 免除はそのトークンだけで、DB の mfa_enabled は変えない。パスワードでログインしたトークンは従来どおり。
    use super::*;

    fn user(is_staff: bool, mfa_enabled: bool) -> User {
        User {
            id: 1,
            email: "t@example.invalid".to_string(),
            password: String::new(),
            username: "t".to_string(),
            is_active: true,
            is_staff,
            mfa_enabled,
            can_view_all_payroll: false,
            can_view_all_expenses: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn employee_profile() -> UserProfile {
        UserProfile { id: 1, user_id: 1, partner_id: None, employee_id: Some(5), is_first_login: false }
    }

    fn partner_profile() -> UserProfile {
        UserProfile { id: 1, user_id: 1, partner_id: Some("P1".to_string()), employee_id: None, is_first_login: false }
    }

    #[test]
    fn パスワードのログインでは_mfa_未登録の管理者と社員に登録を求める() {
        assert!(AuthUser::from_user(user(true, false), None).requires_mfa_enrollment());
        assert!(AuthUser::from_user(user(false, false), Some(employee_profile())).requires_mfa_enrollment());
    }

    #[test]
    fn sso_のログインでは_mfa_未登録でも登録を求めない() {
        let admin = AuthUser::from_user(user(true, false), None).with_sso_login(true);
        let emp = AuthUser::from_user(user(false, false), Some(employee_profile())).with_sso_login(true);
        assert!(!admin.requires_mfa_enrollment());
        assert!(!emp.requires_mfa_enrollment());
        assert!(admin.is_sso_login() && emp.is_sso_login());
    }

    #[test]
    fn 免除は_トークンだけ_db_の_mfa_enabled_は変わらない() {
        // 同じ利用者(mfa_enabled=false)が、SSO では免除、パスワードでは要求。DB の値は同じまま。
        let via_sso = AuthUser::from_user(user(true, false), None).with_sso_login(true);
        let via_password = AuthUser::from_user(user(true, false), None);
        assert!(!via_sso.user.mfa_enabled && !via_password.user.mfa_enabled);
        assert!(!via_sso.requires_mfa_enrollment());
        assert!(via_password.requires_mfa_enrollment());
    }

    #[test]
    fn mfa_登録済みなら_どちらの方式でも要求しない() {
        assert!(!AuthUser::from_user(user(true, true), None).requires_mfa_enrollment());
        assert!(!AuthUser::from_user(user(true, true), None).with_sso_login(true).requires_mfa_enrollment());
    }

    #[test]
    fn パートナーは_もともと対象外() {
        assert!(!AuthUser::from_user(user(false, false), Some(partner_profile())).requires_mfa_enrollment());
    }

    #[test]
    fn エンジニア直接認証は_sso_の印を持たない() {
        let e = AuthUser::from_engineer(9, "e".to_string(), "e@example.invalid".to_string(), None);
        assert!(!e.is_sso_login());
        assert!(!e.requires_mfa_enrollment());
    }
}
