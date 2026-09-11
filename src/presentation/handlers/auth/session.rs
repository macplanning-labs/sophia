use crate::config::AppState;
use crate::domain::services::auth_jwt;
use crate::domain::services::legacy_hash::BcryptVerifier;
use crate::infrastructure::repositories::auth_repo;
use crate::infrastructure::repositories::jwt_blacklist_repo::JwtBlacklistRepo;
use crate::infrastructure::repositories::user_repo;
use crate::presentation::cookie_util;
use crate::presentation::login_guard;
use crate::presentation::middleware::role::AuthUser;
use auth_core::domain::jwt::TokenBlacklist;
use auth_core::domain::password::verify_and_needs_rehash;
use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
    Extension,
};
use axum_extra::extract::cookie::CookieJar;
use sqlx::PgPool;
use time::Duration;

// ── ログイン ──

/// JWT Cookieの中身をJWTブラックリストに登録する（ログアウト共通処理）。
/// デコードに失敗した場合（期限切れ・不正等）は何もしない（どのみち以後使えないトークンのため）。
async fn blacklist_jwt_cookie(pool: &PgPool, secret: &str, token: &str) {
    let Ok((claims, extra)) = auth_jwt::decode_sophia_claims(token, secret) else {
        return;
    };
    let Some(expires_at) = chrono::DateTime::<chrono::Utc>::from_timestamp(claims.exp, 0) else {
        return;
    };
    let repo = JwtBlacklistRepo(pool.clone());
    if let Err(_e) = repo.blacklist(&extra.jti, expires_at).await {
        tracing::error!(
            "[認証/ログアウト] 処理=blacklist 結果=失敗 影響=当該jtiがログアウト後も期限内有効のまま | jti_len={}",
            extra.jti.len()
        );
    }
}

/// ログアウト（GET /logout）
pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    let pool = &state.pool;

    if let Some(session_cookie) = jar.get("sophia_session") {
        let sid = session_cookie.value().to_string();
        if let Ok(Some((uid, email))) = auth_repo::find_session_user_email(pool, &sid).await {
            auth_repo::insert_auth_event(pool, "logout", Some(uid), Some(&email), None, "get")
                .await;
        }
        if let Err(e) = auth_repo::delete_session(pool, &sid).await {
            tracing::error!("DB error: {:?}", e);
        }
    }

    if let Some(jwt_cookie) = jar.get("sophia_jwt") {
        let token = jwt_cookie.value().to_string();
        blacklist_jwt_cookie(pool, &state.secret_key, &token).await;
    }

    if let Some(refresh_cookie) = jar.get("sophia_refresh") {
        let token = refresh_cookie.value().to_string();
        blacklist_jwt_cookie(pool, &state.secret_key, &token).await;
    }

    let cookie_session =
        cookie_util::build_auth_cookie("sophia_session", String::new(), Duration::seconds(0));
    let jar = cookie_util::clear_staff_session_cookies(jar);
    let jar = jar.remove(cookie_session);

    (jar, Redirect::to("/login"))
}

#[derive(serde::Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
}

impl auth_core::domain::attempt_lock::LockKey for LoginForm {
    fn lock_key(&self) -> &str {
        &self.email
    }
}

// ── SPA用 JSON API ──

/// POST /api/auth/login
///
/// 試行回数制限は`routes.rs`の`login_routes`に配線された
/// `auth_core::infrastructure::rate_limit::attempt_lock_middleware`が
/// ハンドラ実行前に判定済み（Step3 Phase5-3）。ハンドラ内では`login_guard`の
/// `login_key`系呼び出しは行わない（`portal_key`/`mfa_key`は引き続き`login_guard.rs`が担当）。
pub async fn api_login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    jar: axum_extra::extract::CookieJar,
    axum::Json(form): axum::Json<LoginForm>,
) -> impl IntoResponse {
    let pool = &state.pool;
    let email = form.email.as_str();
    let password = form.password.as_str();
    let ip = login_guard::client_ip(&headers);

    let user = auth_repo::find_user_for_login(pool, email)
        .await
        .ok()
        .flatten();

    match user {
        Some((id, _email, stored_hash, is_staff, mfa_enabled)) => {
            let verify_result = verify_and_needs_rehash(password, &stored_hash, &[&BcryptVerifier]);
            match verify_result {
                Ok((true, new_hash)) => {
                    // bcryptハッシュだった場合はArgon2へ再ハッシュしてDB更新（Step3 Phase1）
                    if let Some(new_hash) = new_hash {
                        if let Err(e) = user_repo::update_password(pool, id, &new_hash).await {
                            tracing::error!("Argon2再ハッシュの保存に失敗: user_id={} {:?}", id, e);
                        }
                    }

                    auth_repo::insert_auth_event(
                        pool,
                        "login_success",
                        Some(id),
                        Some(email),
                        Some(&ip),
                        "password",
                    )
                    .await;
                    // ロール判定: Admin > Employee > Partner > USER
                    let role = if is_staff {
                        "ADMIN"
                    } else {
                        // プロフィールからロールを判定
                        let profile = match auth_repo::find_user_profile_role_fields(pool, id).await
                        {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::error!("プロフィール取得エラー: {}", e);
                                None
                            }
                        };
                        match profile {
                            Some((_, Some(_))) => "EMPLOYEE",
                            Some((Some(_), _)) => "PARTNER",
                            _ => "USER",
                        }
                    };
                    // 社員は MFA 未登録でもログイン成功させる。フロントは mfa_setup_required で誘導。
                    let mfa_setup_required =
                        (role == "ADMIN" || role == "EMPLOYEE") && !mfa_enabled;

                    if mfa_enabled {
                        // MFA有効: 短命(5分)のmfa_pendingトークンのみ発行。sophia_jwtは発行しない
                        let claims = auth_jwt::issue_mfa_pending_claims(id, role);
                        let token = match auth_jwt::encode_sophia_claims(&claims, &state.secret_key)
                        {
                            Ok(t) => t,
                            Err(e) => {
                                tracing::error!(
                                    "mfa_pendingトークン発行に失敗: user_id={} {:?}",
                                    id,
                                    e
                                );
                                return internal_error_response();
                            }
                        };
                        let cookie = cookie_util::sophia_mfa_pending_header(&token, 300);
                        (
                            axum::http::StatusCode::OK,
                            [(axum::http::header::SET_COOKIE, cookie)],
                            axum::Json(serde_json::json!({
                                "success": true,
                                "role": role,
                                "mfa_required": true,
                                "mfa_setup_required": false
                            })),
                        )
                            .into_response()
                    } else {
                        // MFA無効: access + refresh トークンペアを発行
                        let pair = match auth_jwt::issue_and_encode_token_pair(id, &role, &state.secret_key)
                        {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::error!(
                                    "トークンペア発行に失敗: user_id={} {:?}",
                                    id,
                                    e
                                );
                                return internal_error_response();
                            }
                        };
                        let jar = cookie_util::add_staff_session_cookies(jar, pair.access, pair.refresh);
                        (
                            jar,
                            axum::Json(serde_json::json!({
                                "success": true,
                                "role": role,
                                "mfa_required": false,
                                "mfa_setup_required": mfa_setup_required
                            })),
                        )
                            .into_response()
                    }
                }
                Ok((false, _)) => fail_login(pool, &ip, email).await,
                Err(e) => {
                    tracing::error!("パスワード検証エラー: user_id={} {:?}", id, e);
                    fail_login(pool, &ip, email).await
                }
            }
        }
        None => fail_login(pool, &ip, email).await,
    }
}

async fn fail_login(pool: &sqlx::PgPool, ip: &str, email: &str) -> axum::response::Response {
    tracing::warn!("login failure ip={} email={}", ip, email);
    auth_repo::insert_auth_event(pool, "login_fail", None, Some(email), Some(ip), "password").await;
    (
        axum::http::StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({
            "success": false,
            "error": login_guard::LOGIN_FAIL_MSG
        })),
    )
        .into_response()
}

fn internal_error_response() -> axum::response::Response {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        axum::Json(serde_json::json!({ "success": false, "error": "ログイン処理に失敗しました" })),
    )
        .into_response()
}

/// POST /api/auth/logout
pub async fn api_logout(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    jar: axum_extra::extract::CookieJar,
) -> impl IntoResponse {
    let pool = &state.pool;
    let ip = login_guard::client_ip(&headers);

    if let Some(session_cookie) = jar.get("sophia_session") {
        let sid = session_cookie.value().to_string();
        if let Ok(Some((uid, email))) = auth_repo::find_session_user_email(pool, &sid).await {
            auth_repo::insert_auth_event(pool, "logout", Some(uid), Some(&email), Some(&ip), "api")
                .await;
        }
        if let Err(e) = auth_repo::delete_session(pool, &sid).await {
            tracing::error!("DB error on api_logout: {:?}", e);
        }
    }

    if let Some(jwt_cookie) = jar.get("sophia_jwt") {
        let token = jwt_cookie.value().to_string();
        blacklist_jwt_cookie(pool, &state.secret_key, &token).await;
    }

    if let Some(refresh_cookie) = jar.get("sophia_refresh") {
        let token = refresh_cookie.value().to_string();
        blacklist_jwt_cookie(pool, &state.secret_key, &token).await;
    }

    let cookie_session =
        cookie_util::build_auth_cookie("sophia_session", String::new(), time::Duration::seconds(0));
    let jar = cookie_util::clear_staff_session_cookies(jar);
    let jar = jar.remove(cookie_session);
    (jar, axum::Json(serde_json::json!({ "success": true }))).into_response()
}

/// GET /api/auth/me
pub async fn api_me(Extension(auth_user): Extension<AuthUser>) -> impl IntoResponse {
    axum::Json(serde_json::json!({
        "user_id": auth_user.user.id,
        "role": auth_user.role().as_str(),
        "email": auth_user.user.email,
        "can_view_all_expenses": auth_user.can_view_all_expenses(),
        "can_view_all_payroll": auth_user.can_view_all_payroll(),
        "mfa_enabled": auth_user.user.mfa_enabled,
        "mfa_setup_required": auth_user.requires_mfa_enrollment(),
    }))
}
