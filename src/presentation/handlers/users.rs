/// presentation/handlers/users.rs — ユーザー管理 CRUD（Admin専用）
///
/// ## エンドポイント
/// - GET  /users                    — 一覧
/// - GET  /users/new                — 新規作成フォーム
/// - POST /users                    — 作成
/// - GET  /users/{id}               — 詳細
/// - GET  /users/{id}/edit          — 編集フォーム
/// - POST /users/{id}               — 更新
/// - POST /users/{id}/toggle-active — アクティブ切替

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Extension, Form,
};
use sqlx::PgPool;

use crate::infrastructure::repositories::user_repo::{self, UserRow};
use crate::presentation::middleware::role::AuthUser;
use crate::domain::services::password_policy;
use crate::infrastructure::repositories::auth_repo;


// ── フォームデータ ──

/// フォームデータ
#[derive(Debug, serde::Deserialize)]
pub struct UserForm {
    pub email: String,
    pub username: String,
    pub password: Option<String>,
    pub is_staff: Option<String>,
    pub can_view_all_payroll: Option<String>,
    pub can_view_all_expenses: Option<String>,
    pub employee_id: Option<i64>,
    pub partner_id: Option<String>,
}

// ── ハンドラ ──

/// POST /users — 作成
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<UserForm>,
) -> impl IntoResponse {
    let password = form.password.unwrap_or_default();
    if password.is_empty() || form.email.is_empty() {
        return Redirect::to("/users/new");
    }
    if password_policy::validate(&password).is_err() {
        return Redirect::to("/users/new");
    }

    let hashed = match auth_core::domain::password::hash_password(&password) {
        Ok(h) => h,
        Err(_) => return Redirect::to("/users/new"),
    };

    let is_staff = form.is_staff.is_some();

    // ユーザー作成
    let user_id = match user_repo::insert_user(&pool, &form.email, &hashed, &form.username, is_staff).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("ユーザー作成エラー: {e}");
            return Redirect::to("/users/new");
        }
    };

    // プロフィール作成（employee_id / partner_id 紐付け）
    let employee_id = form.employee_id.filter(|&id| id > 0);
    let partner_id = form.partner_id.filter(|s| !s.is_empty());

    if let Err(e) = user_repo::insert_user_profile(&pool, user_id, partner_id.as_deref(), employee_id, true).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/users/{}", user_id))
}

/// POST /users/{id} — 更新
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<UserForm>,
) -> impl IntoResponse {
    let is_staff = form.is_staff.is_some();
    let can_view_all_payroll = form.can_view_all_payroll.is_some();
    let can_view_all_expenses = form.can_view_all_expenses.is_some();

    // ユーザー基本情報更新
    if let Err(e) = user_repo::update_user_basic(&pool, id, &form.email, &form.username, is_staff, can_view_all_payroll, can_view_all_expenses).await {
        tracing::error!("DB error: {:?}", e);
    }

    // パスワード変更（入力がある場合のみ）
    if let Some(ref pw) = form.password {
        if !pw.is_empty() {
            if let Ok(hashed) = auth_core::domain::password::hash_password(pw) {
                if let Err(e) = user_repo::update_password(&pool, id, &hashed).await {
                    tracing::error!("DB error: {:?}", e);
                }
            }
        }
    }

    // プロフィール更新（employee_id / partner_id）
    let employee_id = form.employee_id.filter(|&eid| eid > 0);
    let partner_id = form.partner_id.filter(|s| !s.is_empty());

    if let Err(e) = user_repo::upsert_user_profile(&pool, id, partner_id.as_deref(), employee_id, false).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/users/{}", id))
}

/// POST /users/{id}/toggle-active — アクティブ切替
pub async fn toggle_active(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if let Err(e) = user_repo::toggle_active(&pool, id).await {
        tracing::error!("DB error: {:?}", e);
    }

    Redirect::to(&format!("/users/{}", id))
}

// SPA用 JSON API
pub async fn api_index(State(pool): State<PgPool>) -> axum::Json<Vec<UserRow>> {
    let users = user_repo::list_user_rows(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("users api: {:?}", e); vec![] });
    axum::Json(users)
}

/// GET /api/users/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user = user_repo::find_user_detail(&pool, id).await.ok().flatten();

    match user {
        Some(u) => axum::Json(serde_json::json!(u)).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ── SPA用 JSON API: CRUD ──

/// POST /api/users — 作成（JSON）
#[derive(Debug, serde::Deserialize)]
pub struct ApiUserForm {
    pub email: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub is_staff: bool,
    pub employee_id: Option<i64>,
    pub partner_id: Option<String>,
}

pub async fn api_create(
    State(pool): State<PgPool>,
    axum::Json(form): axum::Json<ApiUserForm>,
) -> impl IntoResponse {
    if form.email.is_empty() || form.password.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "メールアドレスとパスワードは必須です"}))).into_response();
    }
    if let Err(msg) = password_policy::validate(&form.password) {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": msg}))).into_response();
    }

    // メール重複チェック
    let exists = user_repo::count_by_email(&pool, &form.email).await.unwrap_or(0);
    if exists > 0 {
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": "このメールアドレスは既に登録されています"}))).into_response();
    }

    let hashed = match auth_core::domain::password::hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": "パスワードのハッシュ化に失敗しました"}))).into_response(),
    };

    let user_id = match user_repo::insert_user(&pool, &form.email, &hashed, &form.username, form.is_staff).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("ユーザー作成エラー: {e}");
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"error": format!("作成に失敗しました: {e}")}))).into_response();
        }
    };

    // プロフィール作成
    let employee_id = form.employee_id.filter(|&id| id > 0);
    let partner_id = form.partner_id.filter(|s| !s.is_empty());
    if let Err(e) = user_repo::insert_user_profile(&pool, user_id, partner_id.as_deref(), employee_id, true).await {
        let msg = e.to_string();
        let friendly = if msg.contains("idx_user_profile_employee_id") {
            "この社員は既に別のユーザーに紐付けられています".to_string()
        } else {
            format!("ユーザーは作成されましたが、社員/パートナーの紐付けに失敗しました: {e}")
        };
        tracing::error!("プロフィール作成エラー: {e}");
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": friendly, "id": user_id}))).into_response();
    }

    axum::Json(serde_json::json!({"success": true, "id": user_id})).into_response()
}

/// PUT /api/users/{id} — 更新（JSON）
#[derive(Debug, serde::Deserialize)]
pub struct ApiUserUpdateForm {
    pub email: String,
    pub username: String,
    #[serde(default)]
    pub is_staff: bool,
    #[serde(default)]
    pub can_view_all_payroll: bool,
    #[serde(default)]
    pub can_view_all_expenses: bool,
    pub employee_id: Option<i64>,
    pub partner_id: Option<String>,
}

pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(form): axum::Json<ApiUserUpdateForm>,
) -> impl IntoResponse {
    if form.email.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "メールアドレスは必須です"}))).into_response();
    }

    // メール重複チェック（自分以外）
    let exists = user_repo::count_by_email_excluding(&pool, &form.email, id).await.unwrap_or(0);
    if exists > 0 {
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": "このメールアドレスは既に使用されています"}))).into_response();
    }

    if let Err(e) = user_repo::update_user_basic(&pool, id, &form.email, &form.username, form.is_staff, form.can_view_all_payroll, form.can_view_all_expenses).await {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": format!("更新に失敗しました: {e}")}))).into_response();
    }

    // プロフィール更新
    let employee_id = form.employee_id.filter(|&eid| eid > 0);
    let partner_id = form.partner_id.filter(|s| !s.is_empty());
    if let Err(e) = user_repo::upsert_user_profile(&pool, id, partner_id.as_deref(), employee_id, false).await {
        let msg = e.to_string();
        let friendly = if msg.contains("idx_user_profile_employee_id") {
            "この社員は既に別のユーザーに紐付けられています".to_string()
        } else {
            format!("基本情報は更新されましたが、社員/パートナーの紐付けに失敗しました: {e}")
        };
        tracing::error!("プロフィール更新エラー: {e}");
        return (axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": friendly}))).into_response();
    }

    axum::Json(serde_json::json!({"success": true})).into_response()
}

/// POST /api/users/{id}/toggle — 有効/無効切替（JSON）
pub async fn api_toggle_active(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let result = user_repo::toggle_active_returning(&pool, id).await;

    match result {
        Ok(is_active) => axum::Json(serde_json::json!({"success": true, "is_active": is_active})).into_response(),
        Err(e) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": format!("{e}")}))).into_response(),
    }
}

/// POST /api/users/{id}/reset-password — パスワードリセット（JSON）
#[derive(Debug, serde::Deserialize)]
pub struct PasswordResetForm {
    pub new_password: String,
}

pub async fn api_reset_password(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(form): axum::Json<PasswordResetForm>,
) -> impl IntoResponse {
    if form.new_password.is_empty() {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "パスワードを入力してください"}))).into_response();
    }
    if let Err(msg) = password_policy::validate(&form.new_password) {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": msg}))).into_response();
    }

    let hashed = match auth_core::domain::password::hash_password(&form.new_password) {
        Ok(h) => h,
        Err(_) => return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": "パスワードのハッシュ化に失敗しました"}))).into_response(),
    };

    if let Err(e) = user_repo::update_password(&pool, id, &hashed).await {
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": format!("{e}")}))).into_response();
    }

    // 既存セッションを無効化（乗っ取り対策）
    if let Err(e) = auth_repo::delete_sessions_for_user(&pool, id).await {
        tracing::error!("session revoke after password reset: {:?}", e);
    }
    auth_repo::insert_auth_event(&pool, "password_reset", Some(id), None, None, "admin_api").await;

    axum::Json(serde_json::json!({"success": true})).into_response()
}

/// DELETE /api/users/{id} — 削除（JSON）
///
/// s_user_profile / s_session / MFA関連テーブルは ON DELETE CASCADE で自動削除される。
/// 各種 *_by_id 監査カラムは ON DELETE SET NULL のため削除をブロックしない。
/// 自分自身の削除は誤操作防止のため禁止する。
pub async fn api_delete(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    if auth_user.user.id == id {
        return (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": "自分自身のアカウントは削除できません"}))).into_response();
    }

    match user_repo::delete_user(&pool, id).await {
        Ok(n) if n > 0 => {
            tracing::info!("ユーザー削除: id={}", id);
            axum::Json(serde_json::json!({"success": true})).into_response()
        }
        Ok(_) => (axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": "ユーザーが見つかりません"}))).into_response(),
        Err(e) => {
            tracing::error!("ユーザー削除エラー: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({"error": format!("削除に失敗しました: {e}")}))).into_response()
        }
    }
}
