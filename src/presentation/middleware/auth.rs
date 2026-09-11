use auth_core::domain::jwt::TokenBlacklist;
/// presentation/middleware/auth.rs — 認証ミドルウェア
///
/// Cookie ベースのセッション認証 + MFA 検証チェック。
/// 未認証リクエストは /login にリダイレクト。
/// MFA 有効ユーザーで MFA 未検証の場合は /mfa にリダイレクト（API は 403 JSON）。
/// パートナートークン・招待・ポータルマジックリンクは認証不要（§14: /api/v1/...）。
/// ログイン時にUserProfileも取得し、AuthUser としてExtensionに注入。
/// エンジニアセッション（engineer_id）の場合は m_engineer から直接認証情報を取得。
use axum::{
    extract::State,
    http::{Request, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use sqlx::PgPool;

use super::role::AuthUser;
use crate::config::AppState;
use crate::domain::models::system::{User, UserProfile};

/// staff 認証結果（access + refresh による階層的フォールバック）
pub enum StaffAuthOutcome {
    /// access / refresh 両方有効でユーザーも active
    Authenticated {
        user: AuthUser,
        extra: crate::domain::services::auth_jwt::SophiaExtraClaims,
        /// サイレントリフレッシュされた場合は Some(cookie)
        refreshed_access_cookie: Option<Cookie<'static>>,
    },
    /// access / refresh 両方無効（engineer session へフォールバック可）
    Unauthenticated,
    /// DB 障害など。401 にしない
    Internal,
}

/// 複数 Set-Cookie ヘッダーを応答に追加
fn append_set_cookie(response: &mut Response, cookie: Cookie<'static>) {
    let jar = CookieJar::new().add(cookie);
    for value in jar.into_response().headers().get_all(header::SET_COOKIE) {
        response.headers_mut().append(header::SET_COOKIE, value.clone());
    }
}

/// DB 障害など内部エラー → 401 にしない
fn auth_internal_error_response(path: &str) -> Response {
    tracing::error!("[認証/middleware] 処理=resolve_staff_auth 結果=失敗 影響=認証判定不能のためリクエスト拒否 | 内部エラー");
    if path.starts_with("/api/") {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "error": "サーバーエラーが発生しました。しばらく経ってから再度お試しください。"
            })),
        )
            .into_response()
    } else {
        axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
    }
}

/// 認証ミドルウェア
pub async fn auth_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // トークンURL・招待URL・静的ファイル・ポータル認証は認証不要
    let path = request.uri().path().to_string();

    // 静的アセット・認証不要パスはそのまま通す
    if path.starts_with("/v1/") // 外部向けAPI（APIキー認証）
        || path.starts_with("/api/v1/token/")
        || path.starts_with("/api/v1/invite/")
        || path.starts_with("/api/v1/auth/portal-link/")
        || path.starts_with("/token/") // 画面 + 旧 PDF（--spa）
        || path.starts_with("/invite/") // 招待画面（--spa）
        || path.starts_with("/portal/auth/") // マジックリンク画面（--spa）
        || path.starts_with("/api/v1/auth/passkey/login/")
        || path == "/api/v1/mfa/verify" // TOTP検証。sophia_mfa_pending Cookieでハンドラ自身が認証するため
                                          // auth_middlewareのCookieゲート（sophia_jwt/sophia_session）を通さない
        || path == "/api/v1/mfa/passkey/begin" // パスキーMFA（TOTP版と同じ理由でCookieゲート対象外）
        || path == "/api/v1/mfa/passkey/complete"
        || path == "/api/v1/auth/login"
        || path == "/api/auth/login" // デュアルマウント（廃止予定）
        || path == "/api/v1/auth/logout"
        || path == "/api/auth/logout"
        || path == "/api/v1/auth/portal-login"
        || path == "/api/auth/portal-login"
        || path.starts_with("/static/")
        || path.starts_with("/_next/")         // SPA静的アセット
        || path.starts_with("/portal/login")
        || path == "/login"
        || path.starts_with("/login/")
        || path == "/health"
        || path == "/favicon.ico"
        || path.starts_with("/api/v1/mobile-upload/")
        || path.starts_with("/api/mobile-upload/")
        || path.starts_with("/upload/mobile/")
        || path == "/api/v1/peppol/inbound" // 外部Peppolプロバイダーからのwebhook（共有シークレット認証はハンドラ内で実施）
        || path == "/api/peppol/inbound"
        || path == "/api/v1/webhook/timesheet" // GAS連携webhook（HMAC-SHA256署名検証はハンドラ内で実施）。
                                                 // Phase7-1調査で判明: このバイパスが元々漏れており、
                                                 // 外部（GAS）からの直接呼び出しが常に401になっていた既存バグを合わせて修正
        || path == "/api/webhook/timesheet"
        || path == "/api/v1/mail/webhook" // GAS連携webhook（HMAC-SHA256署名検証はハンドラ内で実施、品質改善P1-3）。
                                            // webhook/timesheetと同種の既存バグ（バイパス漏れで常に401）だった
        || path == "/api/mail/webhook"
        || is_static_asset(&path)
    {
        return next.run(request).await;
    }

    let pool = &state.pool;

    // ① staff/admin: resolve_staff_auth で access + refresh を階層的に検証
    match resolve_staff_auth(pool, &state.secret_key, &jar).await {
        StaffAuthOutcome::Authenticated { user, extra, refreshed_access_cookie } => {
            // MFA 有効ユーザーで MFA 未検証の場合
            if user.user.mfa_enabled && !extra.mfa_verified {
                // MFA検証 API / 画面・ログアウト・me は通す
                if is_mfa_challenge_path(&path) {
                    request.extensions_mut().insert(user);
                    let mut response = next.run(request).await;
                    if let Some(cookie) = refreshed_access_cookie {
                        append_set_cookie(&mut response, cookie);
                    }
                    return response;
                } else if path.starts_with("/api/") {
                    return (
                        axum::http::StatusCode::FORBIDDEN,
                        axum::Json(serde_json::json!({
                            "error": "多要素認証（MFA）の検証が必要です",
                            "mfa_required": true
                        })),
                    )
                        .into_response();
                } else {
                    return Redirect::to("/mfa").into_response();
                }
            } else if user.requires_mfa_enrollment() && !is_mfa_enrollment_path(&path) {
                request.extensions_mut().insert(user);
                return mfa_enrollment_required_response(&path);
            } else {
                request.extensions_mut().insert(user);
                let mut response = next.run(request).await;
                if let Some(cookie) = refreshed_access_cookie {
                    append_set_cookie(&mut response, cookie);
                }
                return response;
            }
        }
        StaffAuthOutcome::Internal => {
            return auth_internal_error_response(&path);
        }
        StaffAuthOutcome::Unauthenticated => {
            // engineer へフォールバック
        }
    }

    // ② engineer: 既存の sophia_session Cookie 経路（無変更）
    if let Some(session_value) = jar.get("sophia_session").map(|c| c.value().to_string()) {
        if let Some(auth_user) = verify_engineer_session(pool, &session_value).await {
            request.extensions_mut().insert(auth_user);
            return next.run(request).await;
        }
        return unauthorized_response(&path);
    }

    unauthorized_response(&path)
}

/// 未認証レスポンス: API は 401 JSON、ページは /login リダイレクト
fn unauthorized_response(path: &str) -> Response {
    use axum::http::StatusCode;

    if path.starts_with("/api/") {
        // API: 401 JSON を返す（fetch が自動追従でHTMLを受け取るのを防止）
        (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "error": "SESSION_EXPIRED",
                "login_url": session_expired_login_url(path)
            })),
        )
            .into_response()
    } else if path.starts_with("/portal") {
        Redirect::to("/portal/login").into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

/// 未認証時のログイン誘導先を、叩かれたAPIパスから判定する。
/// ポータルAPIは /api/v1/portal/... (v1あり) で呼ばれる。/api/portal/... は
/// レガシー二重マウント用で、v1判定が抜けていると常に false になり login_url が
/// 常に /login になってしまうバグがあったため両方を見る。
fn session_expired_login_url(path: &str) -> &'static str {
    let portal = path.starts_with("/api/portal") || path.starts_with("/api/v1/portal");
    if portal {
        "/portal/login"
    } else {
        "/login"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_v1_path_routes_to_portal_login() {
        assert_eq!(
            session_expired_login_url("/api/v1/portal/orders"),
            "/portal/login"
        );
    }

    #[test]
    fn portal_legacy_path_routes_to_portal_login() {
        assert_eq!(
            session_expired_login_url("/api/portal/orders"),
            "/portal/login"
        );
    }

    #[test]
    fn staff_path_routes_to_staff_login() {
        assert_eq!(session_expired_login_url("/api/v1/dashboard"), "/login");
    }

    #[tokio::test]
    async fn verify_staff_jwt_succeeds_for_valid_active_user() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;

        test_support::with_rollback(|pool| async move {
            let (claims, _jti) = auth_jwt::issue_sophia_access_claims(1, "ADMIN", true);
            let token = auth_jwt::encode_sophia_claims(&claims, "test-secret-key").unwrap();

            let result = super::verify_staff_jwt(&pool, "test-secret-key", &token).await;
            assert!(result.is_ok());
            let (auth_user, extra) = result.unwrap();
            assert_eq!(auth_user.user.id, 1);
            assert_eq!(extra.token_type, auth_jwt::TOKEN_TYPE_ACCESS);
        })
        .await;
    }

    #[tokio::test]
    async fn verify_staff_jwt_fails_for_mfa_pending_token_type() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;

        test_support::with_rollback(|pool| async move {
            // mfa_pending トークン（token_type="mfa_pending"）は access として受理されない
            let claims = auth_jwt::issue_mfa_pending_claims(1, "ADMIN");
            let token = auth_jwt::encode_sophia_claims(&claims, "test-secret-key").unwrap();

            let result = super::verify_staff_jwt(&pool, "test-secret-key", &token).await;
            assert!(
                result.is_err(),
                "mfa_pending token must not be accepted as an access token"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn verify_staff_jwt_fails_for_blacklisted_jti() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::jwt_blacklist_repo::JwtBlacklistRepo;
        use crate::infrastructure::repositories::test_support;
        use chrono::Utc;

        test_support::with_rollback(|pool| async move {
            let (claims, jti) = auth_jwt::issue_sophia_access_claims(1, "ADMIN", true);
            let token = auth_jwt::encode_sophia_claims(&claims, "test-secret-key").unwrap();

            // Blacklist the jti
            let blacklist_repo = JwtBlacklistRepo(pool.clone());
            blacklist_repo
                .blacklist(&jti, Utc::now() + chrono::Duration::hours(1))
                .await
                .unwrap();

            let result = super::verify_staff_jwt(&pool, "test-secret-key", &token).await;
            assert!(result.is_err());
        })
        .await;
    }

    #[tokio::test]
    async fn verify_staff_jwt_fails_for_expired_token() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;
        use chrono::Duration;

        test_support::with_rollback(|pool| async move {
            let now = chrono::Utc::now();
            let past_time = (now - Duration::minutes(5)).timestamp();

            let claims = auth_core::domain::jwt::Claims {
                sub: "1".to_string(),
                roles: vec![],
                exp: past_time,
                iat: now.timestamp(),
                iss: auth_jwt::JWT_ISS.to_string(),
                extra: serde_json::to_value(auth_jwt::SophiaExtraClaims {
                    role: "ADMIN".to_string(),
                    mfa_verified: true,
                    jti: uuid::Uuid::new_v4().to_string(),
                    token_type: auth_jwt::TOKEN_TYPE_ACCESS.to_string(),
                })
                .unwrap(),
            };

            let token = auth_jwt::encode_sophia_claims(&claims, "test-secret-key").unwrap();
            let result = super::verify_staff_jwt(&pool, "test-secret-key", &token).await;
            assert!(result.is_err());
            assert!(matches!(
                result,
                Err(auth_core::error::AuthError::TokenExpired)
            ));
        })
        .await;
    }

    #[tokio::test]
    async fn verify_staff_jwt_fails_for_inactive_user() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;

        test_support::with_rollback(|pool| async move {
            // Set user 5 to inactive
            sqlx::query("UPDATE s_user SET is_active = false WHERE id = 5")
                .execute(&pool)
                .await
                .unwrap();

            let (claims, _jti) = auth_jwt::issue_sophia_access_claims(5, "EMPLOYEE", true);
            let token = auth_jwt::encode_sophia_claims(&claims, "test-secret-key").unwrap();

            let result = super::verify_staff_jwt(&pool, "test-secret-key", &token).await;
            assert!(result.is_err());
            assert!(matches!(result, Err(auth_core::error::AuthError::NotFound)));
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_staff_auth_with_valid_refresh_only() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;

        test_support::with_rollback(|pool| async move {
            let pair = auth_jwt::issue_and_encode_token_pair(1, "ADMIN", "test-secret-key").unwrap();
            let jar = CookieJar::new().add(
                crate::presentation::cookie_util::build_auth_cookie(
                    "sophia_refresh",
                    pair.refresh,
                    time::Duration::seconds(604800),
                )
            );

            let result = super::resolve_staff_auth(&pool, "test-secret-key", &jar).await;
            match result {
                StaffAuthOutcome::Authenticated { user, extra, refreshed_access_cookie } => {
                    assert_eq!(user.user.id, 1);
                    assert!(refreshed_access_cookie.is_some(), "Should have refreshed access cookie");
                    assert_eq!(extra.token_type, auth_jwt::TOKEN_TYPE_REFRESH);
                }
                _ => panic!("Should be Authenticated"),
            }
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_staff_auth_with_expired_access_and_valid_refresh() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;
        use chrono::Duration;

        test_support::with_rollback(|pool| async move {
            // Create expired access token
            let now = chrono::Utc::now();
            let past_time = (now - Duration::minutes(5)).timestamp();
            let expired_access = auth_core::domain::jwt::Claims {
                sub: "1".to_string(),
                roles: vec![],
                exp: past_time,
                iat: now.timestamp(),
                iss: auth_jwt::JWT_ISS.to_string(),
                extra: serde_json::to_value(auth_jwt::SophiaExtraClaims {
                    role: "ADMIN".to_string(),
                    mfa_verified: true,
                    jti: uuid::Uuid::new_v4().to_string(),
                    token_type: auth_jwt::TOKEN_TYPE_ACCESS.to_string(),
                })
                .unwrap(),
            };
            let expired_token = auth_jwt::encode_sophia_claims(&expired_access, "test-secret-key").unwrap();

            // Create valid refresh token
            let pair = auth_jwt::issue_and_encode_token_pair(1, "ADMIN", "test-secret-key").unwrap();

            let jar = CookieJar::new()
                .add(crate::presentation::cookie_util::build_auth_cookie(
                    "sophia_jwt",
                    expired_token,
                    time::Duration::seconds(1800),
                ))
                .add(crate::presentation::cookie_util::build_auth_cookie(
                    "sophia_refresh",
                    pair.refresh,
                    time::Duration::seconds(604800),
                ));

            let result = super::resolve_staff_auth(&pool, "test-secret-key", &jar).await;
            match result {
                StaffAuthOutcome::Authenticated { user, extra, refreshed_access_cookie } => {
                    assert_eq!(user.user.id, 1);
                    assert!(refreshed_access_cookie.is_some(), "Should have refreshed access cookie");
                    assert_eq!(extra.token_type, auth_jwt::TOKEN_TYPE_REFRESH);
                }
                _ => panic!("Should be Authenticated with refreshed access"),
            }
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_staff_auth_fails_with_blacklisted_refresh() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::{jwt_blacklist_repo::JwtBlacklistRepo, test_support};
        use chrono::Utc;

        test_support::with_rollback(|pool| async move {
            let pair = auth_jwt::issue_and_encode_token_pair(1, "ADMIN", "test-secret-key").unwrap();

            // Blacklist the refresh token
            let blacklist_repo = JwtBlacklistRepo(pool.clone());
            blacklist_repo
                .blacklist(&pair.refresh_jti, Utc::now() + chrono::Duration::hours(1))
                .await
                .unwrap();

            let jar = CookieJar::new().add(
                crate::presentation::cookie_util::build_auth_cookie(
                    "sophia_refresh",
                    pair.refresh,
                    time::Duration::seconds(604800),
                )
            );

            let result = super::resolve_staff_auth(&pool, "test-secret-key", &jar).await;
            match result {
                StaffAuthOutcome::Unauthenticated => {}
                _ => panic!("Should be Unauthenticated"),
            }
        })
        .await;
    }

    #[tokio::test]
    async fn resolve_staff_auth_fails_when_refresh_in_access_cookie_slot() {
        use crate::domain::services::auth_jwt;
        use crate::infrastructure::repositories::test_support;

        test_support::with_rollback(|pool| async move {
            let pair = auth_jwt::issue_and_encode_token_pair(1, "ADMIN", "test-secret-key").unwrap();

            // Put refresh token in sophia_jwt Cookie slot (wrong token type)
            let jar = CookieJar::new().add(
                crate::presentation::cookie_util::build_auth_cookie(
                    "sophia_jwt",
                    pair.refresh,
                    time::Duration::seconds(1800),
                )
            );

            let result = super::resolve_staff_auth(&pool, "test-secret-key", &jar).await;
            match result {
                StaffAuthOutcome::Unauthenticated => {}
                _ => panic!("Should be Unauthenticated (token_type mismatch)"),
            }
        })
        .await;
    }
}

/// エンジニアセッションを検証（s_session.engineer_id で m_engineer を取得）
async fn verify_engineer_session(pool: &PgPool, session_id: &str) -> Option<AuthUser> {
    let result: Option<(i64, String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT e.id, e.name, e.email, e.partner_id
        FROM m_engineer e
        JOIN s_session s ON e.id = s.engineer_id
        WHERE s.session_id = $1
          AND s.expires_at > NOW()
          AND e.is_active = true
          AND s.engineer_id IS NOT NULL
        "#,
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    result.map(|(engineer_id, name, email, partner_id)| {
        AuthUser::from_engineer(engineer_id, name, email, partner_id)
    })
}

/// ユーザーIDからプロフィールを取得
async fn fetch_profile(pool: &PgPool, user_id: i64) -> Option<UserProfile> {
    sqlx::query_as::<_, UserProfile>(
        "SELECT id, user_id, partner_id, employee_id, is_first_login FROM s_user_profile WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// staff/admin用 access + refresh トークンの検証（階層的フォールバック）
///
/// 1. access を検証。成功したら Authenticated + None
/// 2. access が期限切れ / 破損 → refresh を検証
/// 3. refresh も無効 → Unauthenticated（engineer へフォールバック）
/// 4. DB エラー → Internal（500）
pub async fn resolve_staff_auth(
    pool: &PgPool,
    secret: &str,
    jar: &CookieJar,
) -> StaffAuthOutcome {
    // 1. access を試す
    if let Some(access_token) = jar.get("sophia_jwt").map(|c| c.value().to_string()) {
        match verify_staff_jwt(pool, secret, &access_token).await {
            Ok((user, extra)) => {
                return StaffAuthOutcome::Authenticated {
                    user,
                    extra,
                    refreshed_access_cookie: None,
                };
            }
            Err(auth_core::error::AuthError::NotFound) => {
                // ユーザー無効。refresh を試さない
                return StaffAuthOutcome::Unauthenticated;
            }
            Err(auth_core::error::AuthError::Internal(_)) => {
                return StaffAuthOutcome::Internal;
            }
            Err(auth_core::error::AuthError::TokenExpired) => {
                // 手順 2 へ
            }
            Err(auth_core::error::AuthError::InvalidToken(_)) => {
                // 手順 2 へ
            }
            Err(_) => {
                // その他のエラー（HashError等）は Internal 扱い
                return StaffAuthOutcome::Internal;
            }
        }
    }

    // 2. refresh を試す
    if let Some(refresh_token) = jar.get("sophia_refresh").map(|c| c.value().to_string()) {
        match verify_staff_refresh(pool, secret, &refresh_token).await {
            Ok((user, extra)) => {
                // 3. access のみ再発行
                match crate::domain::services::auth_jwt::issue_and_encode_access(
                    user.user.id,
                    &extra.role,
                    extra.mfa_verified,
                    secret,
                ) {
                    Ok(new_access) => {
                        let cookie = crate::presentation::cookie_util::staff_access_cookie(new_access);
                        return StaffAuthOutcome::Authenticated {
                            user,
                            extra,
                            refreshed_access_cookie: Some(cookie),
                        };
                    }
                    Err(_) => {
                        return StaffAuthOutcome::Internal;
                    }
                }
            }
            Err(auth_core::error::AuthError::Internal(_)) => {
                return StaffAuthOutcome::Internal;
            }
            Err(_) => {
                // blacklist / invalid / expired / user not found → Unauthenticated
                return StaffAuthOutcome::Unauthenticated;
            }
        }
    }

    StaffAuthOutcome::Unauthenticated
}

/// refresh トークンを検証し AuthUser を構築する。
/// access 検証と同じロジックだが、token_type = refresh チェックと
/// extra（role / mfa_verified）をコピーして返す。
async fn verify_staff_refresh(
    pool: &PgPool,
    secret: &str,
    token: &str,
) -> Result<
    (
        AuthUser,
        crate::domain::services::auth_jwt::SophiaExtraClaims,
    ),
    auth_core::error::AuthError,
> {
    // 1. decode + verify signature/exp
    let (claims, extra) = crate::domain::services::auth_jwt::decode_sophia_claims(token, secret)?;

    // 2. token_type = refresh チェック（access / mfa_pending を拒否）
    if extra.token_type != crate::domain::services::auth_jwt::TOKEN_TYPE_REFRESH {
        return Err(auth_core::error::AuthError::InvalidToken(
            "リフレッシュトークンではありません".to_string(),
        ));
    }

    // 3. blacklist check
    let blacklist_repo =
        crate::infrastructure::repositories::jwt_blacklist_repo::JwtBlacklistRepo(pool.clone());
    if blacklist_repo.is_blacklisted(&extra.jti).await? {
        return Err(auth_core::error::AuthError::InvalidToken(
            "リフレッシュトークンは失効済みです".to_string(),
        ));
    }

    // 4. re-fetch user
    let user_id: i64 = claims
        .sub
        .parse()
        .map_err(|_| auth_core::error::AuthError::InvalidToken("subが不正です".to_string()))?;

    let user = fetch_active_staff_user(pool, user_id)
        .await
        .ok_or(auth_core::error::AuthError::NotFound)?;

    // 5. profile + AuthUser
    let profile = fetch_profile(pool, user.id).await;
    Ok((super::role::AuthUser::from_user(user, profile), extra))
}

/// staff/admin用JWTを検証し、AuthUser を構築する。
/// 1. decode_sophia_claims で署名・exp を検証（失敗時 TokenExpired / InvalidToken を返す）
/// 2. token_type が "access" であることを確認（sophia_mfa_pending用トークンの誤用を防止。
///    署名鍵を共用する2種類のトークンのうち、認証済みアクセストークンのみを受理する）
/// 3. jti がブラックリスト（s_jwt_blacklist）に登録されていないか確認
/// 4. sub（user_id）から s_user を再取得（is_active=true を毎回チェック。既存verify_sessionと同条件）
/// 5. UserProfile を取得し AuthUser::from_user() で構築
///
/// 戻り値に SophiaExtraClaims も含めるのは、呼び出し元（auth_middleware）が
/// mfa_verified を見てMFA未検証状態のパス制御（is_mfa_challenge_path）を
/// 従来のセッション方式と同様に適用できるようにするため（AuthUser構造体自体は
/// role.rsの既存定義を変更しない方針のため、ここでタプルとして返す）。
pub async fn verify_staff_jwt(
    pool: &PgPool,
    secret: &str,
    token: &str,
) -> Result<
    (
        AuthUser,
        crate::domain::services::auth_jwt::SophiaExtraClaims,
    ),
    auth_core::error::AuthError,
> {
    // 1. decode + verify signature/exp
    let (claims, extra) = crate::domain::services::auth_jwt::decode_sophia_claims(token, secret)?;

    // 2. token_type チェック
    if extra.token_type != crate::domain::services::auth_jwt::TOKEN_TYPE_ACCESS {
        return Err(auth_core::error::AuthError::InvalidToken(
            "不正なトークン種別です".to_string(),
        ));
    }

    // 3. blacklist check
    let blacklist_repo =
        crate::infrastructure::repositories::jwt_blacklist_repo::JwtBlacklistRepo(pool.clone());
    if blacklist_repo.is_blacklisted(&extra.jti).await? {
        return Err(auth_core::error::AuthError::InvalidToken(
            "トークンは失効済みです".to_string(),
        ));
    }

    // 4. re-fetch user (sub = user_id as string)
    let user_id: i64 = claims
        .sub
        .parse()
        .map_err(|_| auth_core::error::AuthError::InvalidToken("subが不正です".to_string()))?;

    let user = fetch_active_staff_user(pool, user_id)
        .await
        .ok_or(auth_core::error::AuthError::NotFound)?;

    // 5. profile + AuthUser
    let profile = fetch_profile(pool, user.id).await;
    Ok((super::role::AuthUser::from_user(user, profile), extra))
}

/// s_user から is_active=true のユーザーをIDで再取得する（JWT検証用。verify_sessionと同条件）
async fn fetch_active_staff_user(pool: &PgPool, user_id: i64) -> Option<User> {
    #[allow(clippy::type_complexity)]
    let result: Option<(
        i64,
        String,
        String,
        String,
        bool,
        bool,
        bool,
        bool,
        bool,
        chrono::DateTime<chrono::Utc>,
        chrono::DateTime<chrono::Utc>,
    )> = sqlx::query_as(
        "SELECT id, email, password, username, is_active, is_staff, mfa_enabled, \
             can_view_all_payroll, can_view_all_expenses, created_at, updated_at \
             FROM s_user WHERE id = $1 AND is_active = true",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    result.map(
        |(
            id,
            email,
            password,
            username,
            is_active,
            is_staff,
            mfa_enabled,
            can_view_all_payroll,
            can_view_all_expenses,
            created_at,
            updated_at,
        )| {
            User {
                id,
                email,
                password,
                username,
                is_active,
                is_staff,
                mfa_enabled,
                can_view_all_payroll,
                can_view_all_expenses,
                created_at,
                updated_at,
            }
        },
    )
}

/// MFA 未検証セッションがアクセスしてよいパス（検証導線）
fn is_mfa_challenge_path(path: &str) -> bool {
    path.starts_with("/api/v1/mfa/")
        || path == "/mfa"
        || path == "/logout"
        || path == "/api/v1/auth/logout"
        || path == "/api/auth/logout"
        || path == "/api/v1/auth/me"
        || path == "/api/auth/me"
}

/// MFA 未登録の社員がアクセスしてよいパス（登録導線）
fn is_mfa_enrollment_path(path: &str) -> bool {
    path.starts_with("/settings/security") // 画面（Next.js）
        || path.starts_with("/api/v1/settings/security")
        || path.starts_with("/api/v1/security")
        || path.starts_with("/api/security") // デュアル（廃止予定）
        || path.starts_with("/api/v1/mfa/")
        || path == "/mfa"
        || path == "/logout"
        || path == "/api/v1/auth/logout"
        || path == "/api/auth/logout"
        || path == "/api/v1/auth/me"
        || path == "/api/auth/me"
}

fn mfa_enrollment_required_response(path: &str) -> Response {
    if path.starts_with("/api/") {
        return (
            axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({
                "error": "社員アカウントは多要素認証（MFA）の登録が必須です",
                "mfa_setup_required": true
            })),
        )
            .into_response();
    }
    Redirect::to("/settings/security").into_response()
}

/// 静的アセットのパスかどうかを判定
fn is_static_asset(path: &str) -> bool {
    let extensions = [
        ".js", ".css", ".map", ".svg", ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".woff",
        ".woff2", ".ttf", ".eot", ".txt",
    ];
    extensions.iter().any(|ext| path.ends_with(ext))
}
