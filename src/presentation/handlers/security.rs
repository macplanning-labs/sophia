/// presentation/handlers/security.rs — セキュリティ設定ハンドラ
///
/// MFA（TOTP + パスキー）の設定画面。
/// Phase 1: 認証基盤 + MFA の一部。
///
/// ## エンドポイント
/// - GET  /settings/security            — MFA設定一覧
/// - GET  /settings/security/totp/setup  — TOTPセットアップ（QR表示）
/// - POST /settings/security/totp/setup  — TOTPセットアップ確認（6桁コード検証）
/// - POST /settings/security/totp/disable — TOTP無効化
/// - POST /settings/security/passkey/register/begin    — パスキー登録開始（JSON）
/// - POST /settings/security/passkey/register/complete  — パスキー登録完了（JSON）
/// - POST /settings/security/passkey/{id}/delete        — パスキー削除

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Extension, Json,
};
use axum::extract::Path;
use axum_extra::extract::CookieJar;
use sqlx::PgPool;
use uuid::Uuid;

use crate::config::AppState;

use crate::domain::services::webauthn_service;
use crate::infrastructure::repositories::auth_repo;
use crate::infrastructure::repositories::security_repo;
use crate::presentation::cookie_util;
use crate::presentation::middleware::role::AuthUser;

// ── テンプレート定義 ──



/// テンプレートに渡すパスキー情報
#[derive(Debug, Clone)]
pub struct PasskeyInfo {
    pub id: i64,
    pub name: String,
    pub created_at: String,
}

/// フラッシュメッセージ
#[derive(Debug, Clone)]
pub struct FlashMessage {
    pub level: String,
    pub message: String,
}

// ── セキュリティ設定一覧 ──

// ── TOTP セットアップ ──

/// POST /settings/security/totp/begin — 秘密鍵生成 + QRコード発行（JSON API）
///
/// 生成した秘密鍵はまだDBに保存しない。base64化してレスポンスに含め、
/// フロントエンドが確認コードと一緒に /confirm へ送り返す（stateless）。
pub async fn totp_begin(
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let secret_bytes = crate::domain::services::totp_service::generate_secret();
    let qr_base64 = match crate::domain::services::totp_service::generate_qr_base64(&secret_bytes, &auth_user.user.email) {
        Ok(qr) => qr,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    Json(serde_json::json!({
        "secret_b64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &secret_bytes),
        "secret_base32": crate::domain::services::totp_service::secret_to_base32(&secret_bytes),
        "qr_base64": qr_base64,
    })).into_response()
}

#[derive(serde::Deserialize)]
pub struct TotpConfirmForm {
    pub secret_b64: String,
    pub code: String,
}

/// POST /settings/security/totp/confirm — 確認コード検証 → TOTP有効化（JSON API）
pub async fn totp_confirm(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    Json(form): Json<TotpConfirmForm>,
) -> impl IntoResponse {
    let secret_bytes = match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &form.secret_b64) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "不正な秘密鍵です").into_response(),
    };

    match crate::domain::services::totp_service::verify_code(&secret_bytes, &auth_user.user.email, &form.code) {
        Ok(true) => {}
        Ok(false) => return (StatusCode::BAD_REQUEST, "確認コードが正しくありません").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }

    let encrypted = match crate::domain::services::totp_service::encrypt_secret(&secret_bytes, &state.secret_key) {
        Ok(e) => e,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    if let Err(e) = security_repo::create_totp_device(
        &state.pool, auth_user.user.id, &encrypted.ciphertext_b64, &encrypted.nonce_b64,
    ).await {
        tracing::error!("DB error: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "TOTPデバイスの保存に失敗しました").into_response();
    }

    if let Err(e) = security_repo::set_mfa_enabled(&state.pool, auth_user.user.id, true).await {
        tracing::error!("DB error: {:?}", e);
    }

    Json(serde_json::json!({ "ok": true })).into_response()
}

/// POST /settings/security/totp/disable — TOTP無効化
pub async fn totp_disable(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let user_id = auth_user.user.id;
    let is_staff_role = matches!(
        auth_user.role(),
        crate::presentation::middleware::role::Role::Admin
            | crate::presentation::middleware::role::Role::Employee
    );
    let passkey_count = security_repo::count_passkeys(&pool, user_id).await.unwrap_or(0);

    // P5-1e: 社員は最後の MFA 要素を外せない
    if is_staff_role && passkey_count == 0 {
        return (
            StatusCode::BAD_REQUEST,
            "社員アカウントでは多要素認証を無効化できません。別の要素を追加してから削除してください。",
        )
            .into_response();
    }

    // TOTP デバイスを全て無効化
    if let Err(e) = security_repo::deactivate_totp_devices(&pool, user_id).await {
        tracing::error!("DB error: {:?}", e);
    }

    // パスキーが残っていない場合は mfa_enabled を false に
    let passkey_count = security_repo::count_passkeys(&pool, user_id).await.unwrap_or(0);

    if passkey_count == 0 {
        if let Err(e) = security_repo::set_mfa_enabled(&pool, user_id, false).await {
            tracing::error!("DB error: {:?}", e);
        }
    }

    Redirect::to("/settings/security").into_response()
}

// ── パスキー登録 ──

/// POST /settings/security/passkey/register/begin — パスキー登録開始（JSON API）
pub async fn passkey_register_begin(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    let user_id = auth_user.user.id;
    let user_uuid = Uuid::new_v4();

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // 既存パスキーを取得
    let existing = load_passkeys(&state.pool, user_id).await;

    match webauthn_service::start_registration(
        &webauthn,
        user_uuid,
        &auth_user.user.email,
        &auth_user.user.username,
        if existing.is_empty() { None } else { Some(existing) },
    ) {
        Ok((ccr, reg_state)) => {
            // reg_state は s_passkey_login_challenge に保存し、challenge_idをCookieで
            // 中継する（パスワードレスログイン/MFAパスキーと同じ機構）。
            // 旧実装はsophia_sessionセッションIDに紐付けていたが、Step3以降staff/adminは
            // sophia_jwt（Cookieはあるがsophia_sessionは発行されない）のみを持つため、
            // 常にセッションが見つからずパスキー登録が機能していなかった（P0-1）。
            let reg_json = serde_json::to_string(&reg_state).unwrap_or_default();
            let challenge_id = Uuid::new_v4().to_string();
            if let Err(e) =
                auth_repo::insert_passkey_login_challenge(&state.pool, &challenge_id, &reg_json)
                    .await
            {
                tracing::error!("DB error: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "登録セッションの保存に失敗しました").into_response();
            }

            let challenge_cookie = cookie_util::build_auth_cookie(
                "passkey_reg_challenge",
                challenge_id,
                time::Duration::minutes(5),
            );

            (jar.add(challenge_cookie), Json(ccr)).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
        }
    }
}

/// POST /settings/security/passkey/register/complete — パスキー登録完了（JSON API）
pub async fn passkey_register_complete(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthUser>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
    Json(credential): Json<webauthn_rs_proto::RegisterPublicKeyCredential>,
) -> impl IntoResponse {
    let user_id = auth_user.user.id;

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let challenge_id = jar.get("passkey_reg_challenge").map(|c| c.value().to_string());
    let reg_json: Option<String> = if let Some(ref cid) = challenge_id {
        auth_repo::find_passkey_login_challenge(&state.pool, cid).await.ok().flatten()
    } else {
        None
    };
    if let Some(ref cid) = challenge_id {
        if let Err(e) = auth_repo::delete_passkey_login_challenge(&state.pool, cid).await {
            tracing::error!("DB error: {:?}", e);
        }
    }

    let reg_state: webauthn_rs::prelude::PasskeyRegistration = match reg_json {
        Some(json) => match serde_json::from_str(&json) {
            Ok(s) => s,
            Err(_) => return (StatusCode::BAD_REQUEST, "登録セッションが無効です").into_response(),
        },
        None => return (StatusCode::BAD_REQUEST, "登録セッションが見つかりません").into_response(),
    };

    let clear_challenge_cookie = cookie_util::build_auth_cookie(
        "passkey_reg_challenge",
        String::new(),
        time::Duration::seconds(0),
    );

    // 登録完了
    match webauthn_service::finish_registration(&webauthn, &reg_state, &credential) {
        Ok(passkey) => {
            let cred_id = webauthn_service::credential_id_to_string(&passkey);
            let passkey_json = webauthn_service::passkey_to_json(&passkey).unwrap_or_default();

            if let Err(e) = security_repo::insert_webauthn_credential(
                &state.pool, user_id, &cred_id, &passkey_json, "パスキー",
            ).await {
                tracing::error!("DB error: {:?}", e);
            }

            // mfa_enabled を true に更新
            if let Err(e) = security_repo::set_mfa_enabled(&state.pool, user_id, true).await {
                tracing::error!("DB error: {:?}", e);
            }

            (
                jar.remove(clear_challenge_cookie),
                (StatusCode::OK, "パスキーを登録しました"),
            )
                .into_response()
        }
        Err(e) => (
            jar.remove(clear_challenge_cookie),
            (StatusCode::BAD_REQUEST, e.to_string()),
        )
            .into_response(),
    }
}

/// POST /settings/security/passkey/{id}/delete — パスキー削除
pub async fn passkey_delete(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let user_id = auth_user.user.id;
    let is_staff_role = matches!(
        auth_user.role(),
        crate::presentation::middleware::role::Role::Admin
            | crate::presentation::middleware::role::Role::Employee
    );

    let totp_count = security_repo::count_active_totp_devices(&pool, user_id).await.unwrap_or(0);
    let passkey_count = security_repo::count_passkeys(&pool, user_id).await.unwrap_or(0);
    // 最後の1本を消すと MFA 無効になる場合、社員は拒否
    if is_staff_role && totp_count == 0 && passkey_count <= 1 {
        return (
            StatusCode::BAD_REQUEST,
            "社員アカウントでは最後のパスキーを削除できません。別の要素を追加してから削除してください。",
        )
            .into_response();
    }

    // 自分のパスキーのみ削除可能
    if let Err(e) = security_repo::delete_webauthn_credential(&pool, id, user_id).await {
        tracing::error!("DB error: {:?}", e);
    }

    // TOTP もパスキーも無い場合は mfa_enabled を false に
    let totp_count = security_repo::count_active_totp_devices(&pool, user_id).await.unwrap_or(0);
    let passkey_count = security_repo::count_passkeys(&pool, user_id).await.unwrap_or(0);

    if totp_count == 0 && passkey_count == 0 {
        if let Err(e) = security_repo::set_mfa_enabled(&pool, user_id, false).await {
            tracing::error!("DB error: {:?}", e);
        }
    }

    Redirect::to("/settings/security").into_response()
}

// ── ヘルパー ──

/// TOTP セットアップ確認フォーム（QR表示時に hidden で secret_b32 を持ち回る）
#[derive(Debug, serde::Deserialize)]
pub struct TotpSetupConfirmForm {
    pub secret_b32: String,
    pub code: String,
}

/// ユーザーの全パスキーを取得
async fn load_passkeys(pool: &PgPool, user_id: i64) -> Vec<webauthn_rs::prelude::Passkey> {
    let jsons = security_repo::list_passkey_jsons(pool, user_id)
        .await
        .unwrap_or_else(|e| { tracing::warn!("security: fetch_all failed: {:?}", e); vec![] });

    jsons.iter()
        .filter_map(|json| webauthn_service::passkey_from_json(json).ok())
        .collect()
}

// ── SPA用 JSON API ──

/// GET /api/security — MFA設定情報（JSON）
pub async fn api_index(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let user_id = auth_user.user.id;

    let totp_active = security_repo::count_active_totp_devices(&pool, user_id).await.unwrap_or(0) > 0;

    let rows = security_repo::list_passkey_summaries(&pool, user_id)
        .await
        .unwrap_or_else(|e| { tracing::warn!("security api: {:?}", e); vec![] });

    let passkeys: Vec<serde_json::Value> = rows.into_iter().map(|row| {
        serde_json::json!({
            "id": row.id,
            "name": if row.name.is_empty() { "パスキー".to_string() } else { row.name },
            "created_at": row.created_at.format("%Y-%m-%d %H:%M").to_string(),
        })
    }).collect();

    axum::Json(serde_json::json!({
        "user": {
            "email": auth_user.user.email,
            "username": auth_user.user.username,
            "mfa_enabled": auth_user.user.mfa_enabled,
        },
        "totp_active": totp_active,
        "passkeys": passkeys,
        "mfa_setup_required": auth_user.requires_mfa_enrollment(),
        "mfa_required_for_role": matches!(
            auth_user.role(),
            crate::presentation::middleware::role::Role::Admin
                | crate::presentation::middleware::role::Role::Employee
        ),
    }))
}

