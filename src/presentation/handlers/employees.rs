/// presentation/handlers/employees.rs — 社員管理 CRUD
///
/// Phase 4-D: 社員マスタの管理。
///
/// ## エンドポイント
/// - GET  /employees                — 一覧
/// - GET  /employees/new            — 新規作成フォーム
/// - POST /employees                — 作成
/// - GET  /employees/{id}           — 詳細（給与設定・保険加入状況）
/// - GET  /employees/{id}/edit      — 編集フォーム
/// - POST /employees/{id}           — 更新

use axum::{
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    Form, Extension, http::StatusCode, Json,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use crate::domain::authz::redact::redact_employee_salary;
use crate::domain::authz::Authz;
use crate::domain::models::payroll::{Employee, EmployeeForm};
use crate::infrastructure::repositories::employee_repo;
use crate::presentation::handlers::org_admin;
use crate::presentation::middleware::role::AuthUser;

// ── 社員情報(給与設定を含む)の閲覧範囲 ──

/// 社員情報(給与設定を含む)の閲覧範囲
#[derive(Debug, PartialEq, Eq)]
pub enum EmployeeScope {
    /// 全社員(管理者、給与の全件閲覧スイッチ、またはロールの `hr.employee:view`(全社)を持つ人)
    All,
    /// 本人(m_employee.id)のみ
    Own(i64),
    /// 閲覧不可
    Denied,
}

pub fn employee_scope(auth: &AuthUser, authz: Option<&Authz>) -> EmployeeScope {
    if auth.is_admin() || auth.can_see_all_employees(authz) {
        return EmployeeScope::All;
    }
    match auth.employee_id() {
        Some(id) => EmployeeScope::Own(id),
        None => EmployeeScope::Denied,
    }
}

/// 他の社員の分に、給与の金額(基本給・手当・標準報酬月額)を含めてよいか。
/// 管理者、給与の全件閲覧スイッチ、またはロールの `payroll.slip:view`(全社)を持つ人だけ。
/// 人事は全社員の社員情報を見られるが、給与の金額は見せない(U8)。
fn may_see_salary(auth: &AuthUser, authz: Option<&Authz>) -> bool {
    auth.is_admin() || auth.can_see_all_payroll(authz)
}

/// 社員1人分をJSONにする。他人の分で、給与の金額を見せない人には、金額の列を `null` にする。
/// 本人の分は常にそのまま(従来どおり)。
fn employee_json(e: &Employee, caller_employee_id: Option<i64>, salary_visible: bool) -> serde_json::Value {
    let mut v = serde_json::json!(e);
    if !salary_visible && caller_employee_id != Some(e.id) {
        redact_employee_salary(&mut v);
    }
    v
}

fn forbidden() -> Response {
    (StatusCode::FORBIDDEN, Json(serde_json::json!({ "success": false, "error": "権限がありません" }))).into_response()
}

fn employee_code_required() -> Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "success": false, "error": "社員コードは必須です" }))).into_response()
}

// ── テンプレート ──




// ── ハンドラ ──

/// POST /employees — 作成
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<EmployeeForm>,
) -> impl IntoResponse {
    let result = employee_repo::insert(&pool, &form).await;

    match result {
        Ok(id) => Redirect::to(&format!("/employees/{}", id)),
        Err(_) => Redirect::to("/employees/new"),
    }
}

/// POST /employees/{id} — 更新
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<EmployeeForm>,
) -> impl IntoResponse {
    if let Err(e) = employee_repo::update(&pool, id, &form).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/employees/{}", id))
}

// SPA用 JSON API

/// POST /api/employees — 作成（JSON）
pub async fn api_create(
    Extension(auth_user): Extension<AuthUser>,
    State(pool): State<PgPool>,
    Json(form): Json<EmployeeForm>,
) -> Response {
    // ── ハンドラ側の二重の防御。ルートは admin_routes に移すが、置き間違いに備える ──
    if !auth_user.is_admin() {
        return forbidden();
    }

    if form.employee_id.trim().is_empty() {
        return employee_code_required();
    }

    let result = employee_repo::insert(&pool, &form).await;

    match result {
        Ok(id) => Json(serde_json::json!({ "success": true, "id": id })).into_response(),
        Err(e) => {
            tracing::error!("api_create employee: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR,
             Json(serde_json::json!({ "success": false, "error": format!("{e}") }))
            ).into_response()
        }
    }
}

pub async fn api_index(
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    State(pool): State<PgPool>,
) -> Response {
    let authz = authz.map(|e| e.0);
    match employee_scope(&auth_user, authz.as_ref()) {
        EmployeeScope::All => {
            let employees = employee_repo::list_all(&pool)
                .await
                .unwrap_or_else(|e| { tracing::warn!("employees api: {:?}", e); vec![] });
            let salary_visible = may_see_salary(&auth_user, authz.as_ref());
            let me = auth_user.employee_id();
            let rows: Vec<serde_json::Value> = employees.iter().map(|e| employee_json(e, me, salary_visible)).collect();
            Json(rows).into_response()
        }
        EmployeeScope::Own(id) => {
            let emp = employee_repo::find_by_id(&pool, id).await.log_err().ok().flatten();
            match emp {
                Some(e) => Json(vec![e]).into_response(),
                None => {
                    tracing::warn!("employees api: own employee not found, id={}", id);
                    Json::<Vec<Employee>>(vec![]).into_response()
                }
            }
        }
        EmployeeScope::Denied => forbidden(),
    }
}

/// GET /api/employees/{id} — 詳細（JSON）
pub async fn api_detail(
    Extension(auth_user): Extension<AuthUser>,
    authz: Option<Extension<Authz>>,
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Response {
    // ── スコープチェック（All→許可、Own(me)でme==idなら許可、その他は 403） ──
    let authz = authz.map(|e| e.0);
    match employee_scope(&auth_user, authz.as_ref()) {
        EmployeeScope::All => {
            let emp = employee_repo::find_by_id(&pool, id).await.log_err().ok().flatten();
            match emp {
                Some(e) => Json(employee_json(&e, auth_user.employee_id(), may_see_salary(&auth_user, authz.as_ref()))).into_response(),
                None => (StatusCode::NOT_FOUND, "not found").into_response(),
            }
        }
        EmployeeScope::Own(me) => {
            if me == id {
                let emp = employee_repo::find_by_id(&pool, id).await.log_err().ok().flatten();
                match emp {
                    Some(e) => Json(serde_json::json!(e)).into_response(),
                    None => (StatusCode::NOT_FOUND, "not found").into_response(),
                }
            } else {
                forbidden()
            }
        }
        EmployeeScope::Denied => forbidden(),
    }
}

/// PUT /api/employees/{id} — 更新（JSON）
pub async fn api_update(
    Extension(auth_user): Extension<AuthUser>,
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Json(form): Json<EmployeeForm>,
) -> Response {
    // ── ハンドラ側の二重の防御。ルートは admin_routes に移すが、置き間違いに備える ──
    if !auth_user.is_admin() {
        return forbidden();
    }

    if form.employee_id.trim().is_empty() {
        return employee_code_required();
    }

    let result = employee_repo::update(&pool, id, &form).await;

    match result {
        Ok(_) => Json(serde_json::json!({ "success": true })).into_response(),
        Err(e) => {
            tracing::error!("api_update employee: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR,
             Json(serde_json::json!({ "success": false, "error": format!("{e}") }))
            ).into_response()
        }
    }
}

/// DELETE /api/employees/{id} — 論理削除（JSON）
///
/// その社員が担当のデータ(顧客・案件・契約・受注・請求書など)があるときは、本文 `{"transfer_to": {"org_unit_id", "employee_id"}}`
/// で引き継ぎ先を指定する必要がある(無ければ409で件数を返し、何も変えない。D15)。
/// 指定があれば、同じトランザクションで担当を付け替えてから無効にする。担当データが無ければ、本文なしで従来どおり。
pub async fn api_delete(
    Extension(auth_user): Extension<AuthUser>,
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    body: Bytes,
) -> Response {
    // ── ハンドラ側の二重の防御。ルートは admin_routes に移すが、置き間違いに備える ──
    if !auth_user.is_admin() {
        return forbidden();
    }
    let transfer_to = match org_admin::transfer::parse_deactivate_body(&body) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    match org_admin::transfer::deactivate_employee(&pool, auth_user.user.id, id, transfer_to.as_ref()).await {
        Ok(moved) => {
            tracing::info!("社員論理削除: id={}", id);
            let mut resp = serde_json::json!({ "success": true });
            if let Some(updated) = moved {
                resp["transferred"] = serde_json::json!({ "updated": updated, "total": updated.iter().map(|c| c.count).sum::<i64>() });
            }
            Json(resp).into_response()
        }
        Err(e) => e.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::system::{User, UserProfile};
    use chrono::Utc;

    /// テスト用認証ユーザーを作成するヘルパー
    fn auth(is_staff: bool, can_view_all_payroll: bool, employee_id: Option<i64>) -> AuthUser {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            password: "".to_string(),
            username: "testuser".to_string(),
            is_active: true,
            is_staff,
            mfa_enabled: false,
            can_view_all_payroll,
            can_view_all_expenses: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let profile = if is_staff || employee_id.is_some() {
            Some(UserProfile {
                id: 1,
                user_id: 1,
                partner_id: None,
                employee_id,
                is_first_login: false,
            })
        } else {
            None
        };
        AuthUser::from_user(user, profile)
    }

    #[test]
    fn test_employee_scope_admin() {
        let auth = auth(true, false, None);
        assert_eq!(employee_scope(&auth, None), EmployeeScope::All);
    }

    #[test]
    fn test_employee_scope_with_payroll_view_permission() {
        let auth = auth(false, true, Some(5));
        assert_eq!(employee_scope(&auth, None), EmployeeScope::All);
    }

    #[test]
    fn test_employee_scope_own() {
        let auth = auth(false, false, Some(7));
        assert_eq!(employee_scope(&auth, None), EmployeeScope::Own(7));
    }

    #[test]
    fn test_employee_scope_denied() {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            password: "".to_string(),
            username: "testuser".to_string(),
            is_active: true,
            is_staff: false,
            mfa_enabled: false,
            can_view_all_payroll: false,
            can_view_all_expenses: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let auth = AuthUser::from_user(user, None);
        assert_eq!(employee_scope(&auth, None), EmployeeScope::Denied);
    }

    /// パートナー(partner_id あり・employee_id なし)は、社員情報を一切見られない
    #[test]
    fn test_employee_scope_denied_for_partner() {
        let user = User {
            id: 1,
            email: "partner@example.com".to_string(),
            password: "".to_string(),
            username: "partner".to_string(),
            is_active: true,
            is_staff: false,
            mfa_enabled: false,
            can_view_all_payroll: false,
            can_view_all_expenses: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let profile = UserProfile {
            id: 1,
            user_id: 1,
            partner_id: Some("P001".to_string()),
            employee_id: None,
            is_first_login: false,
        };
        let auth = AuthUser::from_user(user, Some(profile));
        assert_eq!(employee_scope(&auth, None), EmployeeScope::Denied);
    }

    /// 技術者(m_engineer 直接認証)は、社員情報を一切見られない
    #[test]
    fn test_employee_scope_denied_for_engineer() {
        let auth = AuthUser::from_engineer(
            9,
            "技術者".to_string(),
            "eng@example.com".to_string(),
            Some("P001".to_string()),
        );
        assert_eq!(employee_scope(&auth, None), EmployeeScope::Denied);
    }

    #[tokio::test]
    async fn test_api_create_forbidden_for_non_admin() {
        let auth = auth(false, false, Some(5));
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_create(
            Extension(auth),
            State(pool),
            Json(EmployeeForm {
                employee_id: "E001".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        let response = result;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_api_update_forbidden_for_non_admin() {
        let auth = auth(false, false, Some(5));
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_update(
            Extension(auth),
            State(pool),
            Path(1),
            Json(EmployeeForm {
                employee_id: "E001".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        assert_eq!(result.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_api_delete_forbidden_for_non_admin() {
        let auth = auth(false, false, Some(5));
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_delete(
            Extension(auth),
            State(pool),
            Path(1),
            Bytes::new(),
        ).await;

        assert_eq!(result.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_api_detail_forbidden_for_other_employee() {
        let auth = auth(false, false, Some(7));
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        // 別の社員 id=8 にアクセス
        let result = api_detail(
            Extension(auth),
            None,
            State(pool),
            Path(8),
        ).await;

        assert_eq!(result.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_api_index_forbidden_for_denied() {
        let user = User {
            id: 1,
            email: "test@example.com".to_string(),
            password: "".to_string(),
            username: "testuser".to_string(),
            is_active: true,
            is_staff: false,
            mfa_enabled: false,
            can_view_all_payroll: false,
            can_view_all_expenses: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let auth = AuthUser::from_user(user, None);
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_index(
            Extension(auth),
            None,
            State(pool),
        ).await;

        assert_eq!(result.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_api_create_rejects_empty_employee_code() {
        let auth = auth(true, false, None);
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_create(
            Extension(auth),
            State(pool),
            Json(EmployeeForm {
                employee_id: "".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        assert_eq!(result.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_api_create_rejects_whitespace_employee_code() {
        let auth = auth(true, false, None);
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_create(
            Extension(auth),
            State(pool),
            Json(EmployeeForm {
                employee_id: "   ".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        assert_eq!(result.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_api_update_rejects_empty_employee_code() {
        let auth = auth(true, false, None);
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_update(
            Extension(auth),
            State(pool),
            Path(1),
            Json(EmployeeForm {
                employee_id: "".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        assert_eq!(result.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_non_admin_gets_403_before_validation() {
        let auth = auth(false, false, Some(5));
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_create(
            Extension(auth),
            State(pool),
            Json(EmployeeForm {
                employee_id: "".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        assert_eq!(result.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_api_create_employee_code_validation_response_body() {
        let auth = auth(true, false, None);
        let pool = sqlx::PgPool::connect_lazy("postgres://localhost/none").expect("pool");

        let result = api_create(
            Extension(auth),
            State(pool),
            Json(EmployeeForm {
                employee_id: "".to_string(),
                last_name: "Test".to_string(),
                first_name: "User".to_string(),
                last_name_kana: None,
                first_name_kana: None,
                employment_type: None,
                birth_date: None,
                hire_date: None,
                email: None,
                base_salary: None,
                position_allowance: None,
                housing_allowance: None,
                commuting_allowance: None,
                standard_monthly_hours: None,
                standard_remuneration: None,
                dependents_count: None,
            }),
        ).await;

        assert_eq!(result.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(result.into_body(), usize::MAX).await.expect("body");
        let body_str = String::from_utf8(body.to_vec()).expect("utf8");
        assert!(body_str.contains("\"success\":false"));
        assert!(body_str.contains("社員コードは必須です"));
    }

    #[test]
    fn test_router_merge_get_post() {
        // axum 0.8 で GET と POST が重ならないマージが成功することを確認
        // Router::new().route("/x", get(h)).merge(Router::new().route("/x", post(h))) が panic しない
        use axum::{Router, routing::get};

        async fn dummy() {}

        let r1: Router = Router::new().route("/x", get(dummy));
        let r2: Router = Router::new().route("/x", axum::routing::post(dummy));
        let _merged = r1.merge(r2);
        // panic しなかったら成功
    }
}
