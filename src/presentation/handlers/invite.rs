/// presentation/handlers/invite.rs — パートナー招待ハンドラ
///
/// パートナーをWebAuthn（パスキー）で招待し、パスワードレスアカウントを作成する。
/// Phase 1: 認証基盤 + MFA の一部。
///
/// ## フロー
/// 1. POST /partners/{id}/invite        — 管理者が招待メール送信
/// 2. GET  /invite/{uuid}               — パートナーが招待URLにアクセス → パスキー登録画面
/// 3. POST /invite/{uuid}/register/begin    — パスキー登録開始（JSON）
/// 4. POST /invite/{uuid}/register/complete — パスキー登録完了（JSON）→ ユーザー作成

use crate::presentation::cookie_util;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Extension, Form, Json,
};
use chrono::Utc;
use uuid::Uuid;

use crate::config::AppState;
use crate::domain::models::mfa::InvitePartnerForm;
use crate::domain::services::webauthn_service;
use crate::infrastructure::repositories::{invite_repo, auth_repo};
use crate::presentation::middleware::role::AuthUser;
use axum_extra::extract::CookieJar;

// ── テンプレート ──


// ── 管理者: 招待メール送信 ──

/// POST /partners/{id}/invite — 招待トークン生成 + メール送信
pub async fn send_invitation(
    State(state): State<AppState>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(partner_id): Path<String>,
    Form(form): Form<InvitePartnerForm>,
) -> impl IntoResponse {
    let pool = &state.pool;

    // 有効期限: 72時間
    let expires_at = Utc::now() + chrono::Duration::hours(72);

    // 招待トークン生成
    let token = Uuid::new_v4();
    let result = invite_repo::insert_invitation(
        pool, &partner_id, token, &form.email, &form.display_name, expires_at,
    ).await;

    match result {
        Ok(_) => {
            // 招待メール送信
            let base_url = std::env::var("BASE_URL")
                .or_else(|_| std::env::var("FRONTEND_URL"))
                .unwrap_or_else(|_| "http://localhost:8110".to_string());
            let invite_url = format!("{}/invite/{}", base_url, token);

            let email_service = crate::domain::services::email_service::EmailService::new(pool.clone());
            let context = crate::domain::services::email_service::compose_invitation_email(
                &form.display_name,
                &invite_url,
            );

            match email_service.send_by_template("INVITE_ENGINEER", &form.email, None, &context).await {
                Ok(_) => tracing::info!("📧 招待メール送信: {} ({})", form.display_name, form.email),
                Err(e) => tracing::error!("📧 招待メール送信失敗: {} - {}", form.email, e),
            }

            Redirect::to(&format!("/partners/{}", partner_id))
        }
        Err(e) => {
            tracing::error!("招待トークン生成エラー: {}", e);
            Redirect::to(&format!("/partners/{}", partner_id))
        }
    }
}

// ── パートナー: 招待受諾 ──

/// GET /api/v1/invite/{uuid}/status — 招待リンク有効性（非消費）
pub async fn api_invite_status(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
) -> impl IntoResponse {
    let token = match Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "valid": false, "error": "無効な招待リンクです" })),
            )
                .into_response();
        }
    };

    let invitation = invite_repo::find_invitation_by_token(&state.pool, token).await.ok().flatten();
    let Some(inv) = invitation else {
        return Json(serde_json::json!({ "valid": false, "error": "招待が見つかりません" })).into_response();
    };

    if inv.is_used {
        let engineer = invite_repo::find_active_engineer_id(&state.pool, &inv.email, &inv.partner_id)
            .await
            .ok()
            .flatten();
        if engineer.is_some() {
            return Json(serde_json::json!({
                "valid": true,
                "already_used": true,
                "display_name": inv.display_name,
                "message": "この招待は使用済みです。ログインを続行できます。"
            }))
            .into_response();
        }
        return Json(serde_json::json!({
            "valid": false,
            "error": "この招待は既に使用されています"
        }))
        .into_response();
    }

    if Utc::now() > inv.expires_at {
        return Json(serde_json::json!({
            "valid": false,
            "error": "招待リンクの有効期限が切れています"
        }))
        .into_response();
    }

    Json(serde_json::json!({
        "valid": true,
        "already_used": false,
        "display_name": inv.display_name,
        "email": inv.email
    }))
    .into_response()
}

/// POST /api/v1/invite/{uuid}/accept — 招待受諾＋セッション作成
pub async fn api_invite_accept(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
) -> impl IntoResponse {
    let token = match Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "success": false, "error": "無効な招待リンクです" })),
            )
                .into_response();
        }
    };

    let invitation = invite_repo::find_invitation_by_token(&state.pool, token).await.ok().flatten();
    let Some(inv) = invitation else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "success": false, "error": "招待が見つかりません" })),
        )
            .into_response();
    };

    let engineer_id = if inv.is_used {
        invite_repo::find_active_engineer_id(&state.pool, &inv.email, &inv.partner_id)
            .await
            .ok()
            .flatten()
    } else {
        if Utc::now() > inv.expires_at {
            return (
                StatusCode::GONE,
                Json(serde_json::json!({
                    "success": false,
                    "error": "招待リンクの有効期限が切れています"
                })),
            )
                .into_response();
        }
        let eid = invite_repo::find_active_engineer_id(&state.pool, &inv.email, &inv.partner_id)
            .await
            .ok()
            .flatten();
        if eid.is_some() {
            if let Err(e) = invite_repo::mark_invitation_used(&state.pool, inv.id).await {
                tracing::error!("招待の使用済み更新に失敗: inv_id={} {:?}", inv.id, e);
            }
            tracing::info!("✅ 招待受諾: {} ({})", inv.display_name, inv.email);
        }
        eid
    };

    let Some(engineer_id) = engineer_id else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "success": false,
                "error": "エンジニアが見つかりません"
            })),
        )
            .into_response();
    };

    let session_id = Uuid::new_v4().to_string();
    if let Err(e) = invite_repo::insert_engineer_session(&state.pool, &session_id, engineer_id).await {
        tracing::error!("エンジニアセッション作成に失敗: engineer_id={} {:?}", engineer_id, e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "success": false,
                "error": "ログイン処理に失敗しました"
            })),
        )
            .into_response();
    }

    let cookie = cookie_util::sophia_session_header(&session_id, 30 * 24 * 60 * 60);
    (
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie)],
        Json(serde_json::json!({
            "success": true,
            "redirect": "/portal/timesheet-entry"
        })),
    )
        .into_response()
}

/// GET /invite/{uuid} — 招待URLアクセス → そのままログイン（マジックリンク・HTMLレガシー）
///
/// パスワード設定不要。招待URLをクリックするだけでセッションが作成され、
/// 稼働報告ページにリダイレクトされる。
pub async fn accept_form(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
) -> impl IntoResponse {
    let token = match Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => return axum::response::Html(
            "<h2>無効な招待リンクです</h2>".to_string()
        ).into_response(),
    };

    // 招待情報取得
    let invitation = invite_repo::find_invitation_by_token(&state.pool, token).await.ok().flatten();

    let invite_repo::InvitationRow { id: inv_id, partner_id, email, display_name, is_used, expires_at } = match invitation {
        Some(inv) => inv,
        None => return axum::response::Html(
            "<h2>招待が見つかりません</h2><p>リンクが無効です。</p>".to_string()
        ).into_response(),
    };

    if is_used {
        // 使用済みでも、エンジニアが存在すればログインさせる
        let engineer = invite_repo::find_active_engineer_id(&state.pool, &email, &partner_id).await.ok().flatten();

        if let Some(engineer_id) = engineer {
            return create_engineer_session_and_redirect(&state.pool, engineer_id).await;
        }
        return axum::response::Html(
            "<h2>この招待は既に使用されています</h2><p><a href='/portal/login'>ログインページへ</a></p>".to_string()
        ).into_response();
    }

    if Utc::now() > expires_at {
        return axum::response::Html(
            "<h2>招待リンクの有効期限が切れています</h2><p>管理者に再発行を依頼してください。</p>".to_string()
        ).into_response();
    }

    // エンジニアを特定
    let engineer = invite_repo::find_active_engineer_id(&state.pool, &email, &partner_id).await.ok().flatten();

    let Some(engineer_id) = engineer else {
        return axum::response::Html(format!(
            "<h2>エンジニアが見つかりません</h2><p>メールアドレス {} に対応するエンジニアが登録されていません。</p>",
            email
        )).into_response();
    };

    // 招待を使用済みに更新（失敗してもログインは継続する。再利用可能な招待が残るだけなので致命的ではない）
    if let Err(e) = invite_repo::mark_invitation_used(&state.pool, inv_id).await {
        tracing::error!("招待の使用済み更新に失敗: inv_id={} {:?}", inv_id, e);
    }

    tracing::info!("✅ 招待受諾: {} ({})", display_name, email);

    // セッション作成 + リダイレクト
    create_engineer_session_and_redirect(&state.pool, engineer_id).await
}

/// エンジニアセッションを作成して稼働報告ページにリダイレクト
async fn create_engineer_session_and_redirect(
    pool: &sqlx::PgPool,
    engineer_id: i64,
) -> axum::response::Response {
    let session_id = uuid::Uuid::new_v4().to_string();

    // セッション作成に失敗した場合、Cookieだけ発行すると「ログインしたのに認証されない」壊れた状態になるため
    // Cookieを発行せずエラー画面を返す
    if let Err(e) = invite_repo::insert_engineer_session(pool, &session_id, engineer_id).await {
        tracing::error!("エンジニアセッション作成に失敗: engineer_id={} {:?}", engineer_id, e);
        return axum::response::Html(
            "<h2>ログイン処理に失敗しました</h2><p>時間をおいて再度お試しください。</p>".to_string()
        ).into_response();
    }

    let cookie = cookie_util::sophia_session_header(&session_id, 30 * 24 * 60 * 60);
    (
        axum::http::StatusCode::SEE_OTHER,
        [
            (axum::http::header::SET_COOKIE, cookie),
            (axum::http::header::LOCATION, "/portal/timesheet-entry".to_string()),
        ],
    ).into_response()
}

/// POST /invite/{uuid}/register — 互換用（マジックリンク方式では使用しない）
///
/// 古い招待フロー（パスワード設定）からの互換。実質的にセッション作成のみ行う。
pub async fn password_register(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
    Json(body): Json<PasswordRegisterRequest>,
) -> impl IntoResponse {
    let token = match Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "無効なトークン"}))).into_response(),
    };

    // パスワードバリデーション
    if body.password.len() < 8 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "パスワードは8文字以上で設定してください"}))).into_response();
    }

    // 招待情報取得
    let invitation = match invite_repo::find_unused_invitation(&state.pool, token).await {
        Ok(inv) => inv,
        Err(e) => {
            tracing::error!("招待情報取得エラー: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "DB error"}))).into_response();
        }
    };

    let invite_repo::UnusedInvitation { id: inv_id, partner_id, email, display_name } = match invitation {
        Some(inv) => inv,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "招待が見つからないか、既に使用済みです"}))).into_response(),
    };

    // パスワードハッシュ
    let password_hash = match auth_core::domain::password::hash_password(&body.password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("パスワードハッシュエラー: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "パスワード処理に失敗しました"}))).into_response();
        }
    };

    // ユーザー + プロフィール作成 + 招待消込（トランザクション）
    match invite_repo::create_partner_user_with_password(
        &state.pool, &email, &password_hash, &display_name, &partner_id, inv_id,
    ).await {
        Ok(_) => {}
        Err(e) => {
            tracing::error!("パートナーアカウント作成エラー: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("アカウント作成エラー: {}", e)}))).into_response();
        }
    }

    tracing::info!("✅ パートナーアカウント作成: {} ({})", display_name, email);
    Json(serde_json::json!({"success": true, "message": "アカウントが作成されました"})).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct PasswordRegisterRequest {
    pub password: String,
}

/// POST /invite/{uuid}/register/begin — パスキー登録開始（JSON API）
pub async fn passkey_register_begin(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    let token = match Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => return (StatusCode::BAD_REQUEST, "無効なトークン").into_response(),
    };

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn RP設定エラー(begin): {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "RP設定エラー").into_response();
        }
    };

    // 招待情報取得
    let invitation = match invite_repo::find_invitation_by_token(&state.pool, token).await {
        Ok(inv) => inv,
        Err(e) => {
            tracing::error!("招待情報取得エラー: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "内部エラー").into_response();
        }
    };

    let invite_repo::InvitationRow {
        id: _inv_id,
        email,
        display_name,
        is_used,
        expires_at,
        ..
    } = match invitation {
        Some(inv) => inv,
        None => return (StatusCode::NOT_FOUND, "招待が見つかりません").into_response(),
    };

    if is_used || Utc::now() > expires_at {
        return (StatusCode::GONE, "招待は無効です").into_response();
    }

    // 仮のUUIDを生成（ユーザーはまだ作成しない）
    let user_uuid = Uuid::new_v4();

    match webauthn_service::start_registration(
        &webauthn,
        user_uuid,
        &email,
        &display_name,
        None,
    ) {
        Ok((ccr, reg_state)) => {
            // reg_state を s_passkey_login_challenge に保存、challenge_id を Cookie に
            let reg_json = match serde_json::to_string(&reg_state) {
                Ok(s) if !s.is_empty() => s,
                Ok(_) | Err(_) => {
                    tracing::error!("reg_state のシリアライズに失敗");
                    return (StatusCode::INTERNAL_SERVER_ERROR, "登録セッション保存エラー").into_response();
                }
            };
            let challenge_id = Uuid::new_v4().to_string();

            if let Err(e) = auth_repo::insert_passkey_login_challenge(&state.pool, &challenge_id, &reg_json).await {
                tracing::error!("チャレンジ保存失敗: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "登録セッション保存エラー").into_response();
            }

            let challenge_cookie = cookie_util::build_auth_cookie(
                "invite_passkey_reg_challenge",
                challenge_id,
                time::Duration::minutes(5),
            );

            (jar.add(challenge_cookie), Json(ccr)).into_response()
        }
        Err(e) => {
            tracing::error!("パスキー登録初期化エラー: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "登録初期化エラー").into_response()
        }
    }
}

/// POST /invite/{uuid}/register/complete — パスキー登録完了 → ユーザー作成（JSON API）
pub async fn passkey_register_complete(
    State(state): State<AppState>,
    Path(token_str): Path<String>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    Json(credential): Json<webauthn_rs_proto::RegisterPublicKeyCredential>,
) -> impl IntoResponse {
    let token = match Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => return (StatusCode::BAD_REQUEST, "無効なトークン").into_response(),
    };

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("WebAuthn RP設定エラー(complete): {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "RP設定エラー").into_response();
        }
    };

    // 招待情報取得
    let invitation = match invite_repo::find_invitation_by_token(&state.pool, token).await {
        Ok(inv) => inv,
        Err(e) => {
            tracing::error!("招待情報取得エラー: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "内部エラー").into_response();
        }
    };

    let invite_repo::InvitationRow {
        id: inv_id,
        partner_id,
        email,
        display_name,
        is_used,
        expires_at,
        ..
    } = match invitation {
        Some(inv) => inv,
        None => return (StatusCode::NOT_FOUND, "招待が見つかりません").into_response(),
    };

    if is_used || Utc::now() > expires_at {
        return (StatusCode::GONE, "招待は無効です").into_response();
    }

    // Cookie クリア用（成功・失敗いずれでも返す）
    let clear_challenge_cookie = cookie_util::build_auth_cookie(
        "invite_passkey_reg_challenge",
        String::new(),
        time::Duration::seconds(0),
    );

    // Cookie から challenge_id を取得 → チャレンジ状態を復元
    let challenge_id = jar.get("invite_passkey_reg_challenge").map(|c| c.value().to_string());
    let reg_json: Option<String> = match challenge_id.as_ref() {
        Some(cid) => match auth_repo::find_passkey_login_challenge(&state.pool, cid).await {
            Ok(v) => v,
            Err(e) => {
                // §3.5: DB失敗を「セッション無し」に偽装しない
                tracing::error!("チャレンジ取得失敗: {:?}", e);
                return (
                    jar.add(clear_challenge_cookie),
                    (StatusCode::INTERNAL_SERVER_ERROR, "内部エラー"),
                )
                    .into_response();
            }
        },
        None => None,
    };

    // チャレンジを削除（成功・失敗いずれでも）
    if let Some(ref cid) = challenge_id {
        if let Err(e) = auth_repo::delete_passkey_login_challenge(&state.pool, cid).await {
            tracing::error!("チャレンジ削除失敗: {:?}", e);
        }
    }

    let reg_state: webauthn_rs::prelude::PasskeyRegistration = match reg_json {
        Some(json) => match serde_json::from_str(&json) {
            Ok(s) => s,
            Err(_) => {
                return (
                    jar.add(clear_challenge_cookie),
                    (StatusCode::BAD_REQUEST, "登録セッション復元エラー"),
                )
                    .into_response();
            }
        },
        None => {
            return (
                jar.add(clear_challenge_cookie),
                (StatusCode::BAD_REQUEST, "登録セッションが見つかりません"),
            )
                .into_response();
        }
    };

    // パスキー登録完了
    let passkey = match webauthn_service::finish_registration(&webauthn, &reg_state, &credential) {
        Ok(pk) => pk,
        Err(e) => {
            tracing::error!("パスキー登録検証エラー: {}", e);
            return (
                jar.add(clear_challenge_cookie),
                (StatusCode::BAD_REQUEST, "登録検証エラー"),
            )
                .into_response();
        }
    };

    // ユーザー + プロフィール + パスキー作成 + 招待消込（トランザクション）
    let cred_id = webauthn_service::credential_id_to_string(&passkey);
    let passkey_json = webauthn_service::passkey_to_json(&passkey).unwrap_or_default();

    if let Err(e) = invite_repo::create_partner_user_with_passkey(
        &state.pool, &email, &display_name, &partner_id, inv_id, &cred_id, &passkey_json,
    ).await {
        tracing::error!("パートナーアカウント作成エラー: {}", e);
        return (jar.add(clear_challenge_cookie), (StatusCode::INTERNAL_SERVER_ERROR, "アカウント作成エラー")).into_response();
    }

    (jar.add(clear_challenge_cookie), (StatusCode::OK, "アカウントが作成されました。ログインしてください。")).into_response()
}
