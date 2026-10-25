use crate::config::AppState;
use crate::domain::services::auth_jwt;
use crate::domain::services::webauthn_service;
use crate::infrastructure::repositories::auth_repo;
use crate::infrastructure::repositories::login_attempt_repo;
use crate::infrastructure::repositories::mfa_attempt_repo::{self, MFA_LOCK_WINDOW_MINUTES, MFA_MAX_ATTEMPTS};
use crate::infrastructure::repositories::security_repo;
use crate::presentation::cookie_util;
use crate::presentation::auth_messages::{client_ip, ClientPeer, LOCKED_MSG};
use auth_core::domain::attempt_lock::{build_lock_key, AttemptStore};
use auth_core::domain::attempt_window::AttemptReservation;
use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use axum_extra::extract::cookie::CookieJar;
use sqlx::PgPool;
use time::Duration;

// ── MFA 検証 ──

/// MFA 検証画面（GET /mfa/verify）
/// SPA版: フロントの /mfa ページにリダイレクト
pub async fn mfa_verify_form() -> impl IntoResponse {
    (
        axum::http::StatusCode::MOVED_PERMANENTLY,
        [(axum::http::header::LOCATION, "/mfa")],
    )
        .into_response()
}

/// TOTP コード検証（POST /mfa/verify）— JSON API
///
/// Step3: `sophia_session`（DBセッション）経由から`sophia_mfa_pending`（短命JWT）経由に変更。
/// 検証成功時は`sophia_jwt`アクセストークン(24h)を発行し`sophia_mfa_pending`を削除する。
/// ログイン試行制限は`AppState.mfa_attempt_store`（`PgAttemptStore`。DB の`s_login_attempt`に記録し、
/// 記録してから数える `reserve`）経由で行う（DEMO-000233 S2-D / 項目6-1）。
/// 社員/管理者のパスワードログインは`routes.rs`の`login_routes`の`login_attempt::guard`が別途担当する。
pub async fn mfa_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: ClientPeer,
    jar: axum_extra::extract::CookieJar,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let pool = &state.pool;
    let code = payload["code"].as_str().unwrap_or_default();

    // sophia_mfa_pending Cookie からJWTを読み取り検証（署名・exp・token_type）
    let pending_token = match jar.get("sophia_mfa_pending") {
        Some(c) => c.value().to_string(),
        None => {
            return axum::Json(
                serde_json::json!({ "success": false, "error": "セッションが無効です" }),
            )
            .into_response()
        }
    };
    let (claims, extra) = match auth_jwt::decode_sophia_claims(&pending_token, &state.secret_key) {
        Ok(v) => v,
        Err(_) => {
            return axum::Json(
                serde_json::json!({ "success": false, "error": "セッションが無効です" }),
            )
            .into_response()
        }
    };
    if extra.token_type != auth_jwt::TOKEN_TYPE_MFA_PENDING {
        return axum::Json(
            serde_json::json!({ "success": false, "error": "セッションが無効です" }),
        )
        .into_response();
    }
    let user_id: i64 = match claims.sub.parse() {
        Ok(v) => v,
        Err(_) => {
            return axum::Json(
                serde_json::json!({ "success": false, "error": "セッションが無効です" }),
            )
            .into_response()
        }
    };
    let role = extra.role.clone();

    let email = match auth_repo::find_email_by_user_id(pool, user_id)
        .await
        .ok()
        .flatten()
    {
        Some(e) => e,
        None => {
            return axum::Json(
                serde_json::json!({ "success": false, "error": "セッションが無効です" }),
            )
            .into_response()
        }
    };

    let ip = client_ip(peer.0, &headers);
    let guard_key = build_lock_key(&login_attempt_repo::ip_group_text(&ip), &user_id.to_string());
    // IP + ユーザーの試行を、記録してから数える(DEMO-000233 S2-D / 項目6-1)。上限を超えていれば、コードを調べずに断る
    match state.mfa_attempt_store.reserve(&guard_key).await {
        Ok(None) => {}
        Ok(Some(retry_after_secs)) => return mfa_store_locked_response(retry_after_secs),
        Err(e) => {
            tracing::error!("試行回数の予約に失敗: {:?}", e);
            return axum::Json(serde_json::json!({ "success": false, "error": "ログイン処理に失敗しました" }))
                .into_response();
        }
    }

    // ユーザー単位の試行の予約(DEMO-000153 S-1)。IP + ユーザーの組の数え方(上)に加えて、**ユーザーごと**に、DB で数える。
    // IP を変えても、ログインし直しても、数は引き継がれる。ロック中は、コードを調べずに断る
    match mfa_attempt_repo::reserve_attempt(
        pool,
        user_id,
        MFA_MAX_ATTEMPTS,
        chrono::Duration::minutes(MFA_LOCK_WINDOW_MINUTES),
    )
    .await
    {
        Ok(r) if r.outcome == AttemptReservation::Locked => {
            tracing::warn!(user_id, "確認の試行が上限に達したため、一時的に止めています");
            return locked_response(r.retry_after_secs.unwrap_or(MFA_LOCK_WINDOW_MINUTES * 60));
        }
        Ok(_) => {}
        Err(e) => {
            // 数えられないときは、通さない(確認を無制限に試せる状態にしない)
            tracing::error!("確認の試行の予約に失敗: {:?}", e);
            return axum::Json(serde_json::json!({ "success": false, "error": "ログイン処理に失敗しました" }))
                .into_response();
        }
    }

    // 暗号化されたTOTP secretを取得
    let totp_row = auth_repo::find_active_totp_device(pool, user_id)
        .await
        .ok()
        .flatten();

    let (device_id, secret_encrypted, nonce) = match totp_row {
        Some(r) => r,
        None => {
            return axum::Json(
                serde_json::json!({ "success": false, "error": "TOTP が設定されていません" }),
            )
            .into_response()
        }
    };

    // SECRET_KEY で復号（AppState.secret_keyを直接利用。従来はstd::env::var再読込＋フォールバック文字列を
    // 使っていたが、State抽出により既に検証済みの値が手に入るためそちらを使う）
    let secret_bytes =
        match crate::domain::services::totp_service::decrypt_secret(
            &secret_encrypted,
            &nonce,
            &state.secret_key,
        ) {
            Ok(b) => b,
            Err(_) => return axum::Json(
                serde_json::json!({ "success": false, "error": "TOTP設定の復号に失敗しました" }),
            )
            .into_response(),
        };

    // TOTP 検証: どの区間で合ったかを調べ、使用済みの区間(同じ・古い)は断る(DEMO-000153 S-1。1 回の UPDATE なので、
    // 同時に同じコードを送られても 1 回しか通らない)
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let code_ok = match auth_core::domain::totp::matching_step(&secret_bytes, code, now_unix) {
        Ok(Some(step)) => mfa_attempt_repo::mark_totp_step_used(pool, device_id, step)
            .await
            .unwrap_or(false),
        _ => false,
    };
    match code_ok {
        true => {
            if let Err(e) = state.mfa_attempt_store.reset(&guard_key).await {
                tracing::error!("試行回数のリセットに失敗: {:?}", e);
            }
            if let Err(e) = mfa_attempt_repo::reset_attempts(pool, user_id).await {
                tracing::error!("ユーザー単位の試行の数の削除に失敗: {:?}", e);
            }
            auth_repo::insert_auth_event(
                pool,
                "mfa_success",
                Some(user_id),
                Some(&email),
                Some(&ip),
                "totp",
            )
            .await;

            // MFA検証成功 → access + refresh トークンペアを発行し、mfa_pendingは削除
            let pair = match auth_jwt::issue_and_encode_token_pair(user_id, &role, &state.secret_key)
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("トークンペア発行に失敗: user_id={} {:?}", user_id, e);
                    return axum::Json(serde_json::json!({ "success": false, "error": "ログイン処理に失敗しました" })).into_response();
                }
            };
            let jar = cookie_util::add_staff_session_cookies(jar, pair.access, pair.refresh);
            let clear_pending = cookie_util::build_auth_cookie(
                "sophia_mfa_pending",
                String::new(),
                Duration::seconds(0),
            );

            (
                jar.remove(clear_pending),
                axum::Json(serde_json::json!({ "success": true })),
            )
                .into_response()
        }
        // 失敗は、上で記録済みの 1 件がそのまま数えになる(次の試行は reserve で上限を判定する)
        _ => {
            auth_repo::insert_auth_event(
                pool,
                "mfa_fail",
                Some(user_id),
                Some(&email),
                Some(&ip),
                "totp",
            )
            .await;
            axum::Json(serde_json::json!({
                "success": false,
                "error": "認証コードが正しくありません"
            }))
            .into_response()
        },
    }
}

// ── パスキー認証（MFA）──

/// POST /mfa/passkey/begin — パスキー認証開始（JSON API）
///
/// Step3: `Extension<AuthUser>`（auth_middleware経由の完全認証前提）から、
/// `sophia_mfa_pending` Cookie（JWT）を自前で検証しuser_idを取得する形に変更。
/// MFA未検証状態ではauth_middlewareが`sophia_jwt`/`sophia_session`いずれも
/// 発行前のためExtension<AuthUser>は使えない（TOTP版mfa_verifyと同じ理由、Phase6-1参照）。
/// WebAuthnチャレンジの中間状態は`s_passkey_login_challenge`テーブル（パスワードレス
/// ログインと同じ機構）を流用し、`mfa_passkey_challenge` Cookieでchallenge_idを渡す
/// （パスワードレスログイン用の`passkey_challenge` Cookieとは名前を分離し、
/// 2つの別フローの中間状態が混線しないようにする）。
pub async fn passkey_auth_begin(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    jar: CookieJar,
) -> impl IntoResponse {
    let pool = &state.pool;

    let user_id = match mfa_pending_user_id(&jar, &state.secret_key) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // ユーザーのパスキーを取得
    let jsons = security_repo::list_passkey_jsons(pool, user_id)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("auth: fetch_all failed: {:?}", e);
            vec![]
        });

    let passkeys: Vec<webauthn_rs::prelude::Passkey> = jsons
        .iter()
        .filter_map(|json| webauthn_service::passkey_from_json(json).ok())
        .collect();

    if passkeys.is_empty() {
        return (StatusCode::BAD_REQUEST, "パスキーが登録されていません").into_response();
    }

    match webauthn_service::start_authentication(&webauthn, &passkeys) {
        Ok((rcr, auth_state)) => {
            // auth_state を s_passkey_login_challenge に保存（5分で期限切れ）
            let challenge_id = uuid::Uuid::new_v4().to_string();
            let auth_json = serde_json::to_string(&auth_state).unwrap_or_default();
            if let Err(e) =
                auth_repo::insert_passkey_login_challenge(pool, &challenge_id, &auth_json).await
            {
                tracing::error!("DB error: {:?}", e);
            }

            let challenge_cookie = cookie_util::build_auth_cookie(
                "mfa_passkey_challenge",
                challenge_id,
                Duration::minutes(5),
            );

            (jar.add(challenge_cookie), Json(rcr)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /mfa/passkey/complete — パスキー認証完了（JSON API）
///
/// Step3: 検証成功時は`sophia_jwt`アクセストークン(24h)を発行し、
/// `sophia_mfa_pending`・`mfa_passkey_challenge`の両Cookieを削除する。
pub async fn passkey_auth_complete(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: ClientPeer,
    jar: CookieJar,
    Json(credential): Json<webauthn_rs::prelude::PublicKeyCredential>,
) -> impl IntoResponse {
    let pool = &state.pool;

    let (user_id, role) = match mfa_pending_user_id_and_role(&jar, &state.secret_key) {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // パスキーによる確認の試行(IP + ユーザー)を、記録してから数える(DEMO-000233 S2-D / 項目6-1)。
    // ロック中は、認証(署名の検証)をせずに断る。失敗は数えとして残り、成功で消す
    let ip = client_ip(peer.0, &headers);
    let mfa_key = build_lock_key(&login_attempt_repo::ip_group_text(&ip), &user_id.to_string());
    match state.passkey_attempt_store.reserve(&mfa_key).await {
        Ok(None) => {}
        Ok(Some(retry_after_secs)) => return mfa_store_locked_response(retry_after_secs),
        Err(e) => {
            tracing::error!("パスキーの試行の予約に失敗: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "ログイン処理に失敗しました").into_response();
        }
    }

    // s_passkey_login_challenge から auth_state を取得（使用済みとして即削除）
    let challenge_id = match jar.get("mfa_passkey_challenge") {
        Some(c) => c.value().to_string(),
        None => return (StatusCode::BAD_REQUEST, "認証セッションが見つかりません").into_response(),
    };
    let auth_json = auth_repo::find_passkey_login_challenge(pool, &challenge_id)
        .await
        .ok()
        .flatten();
    if let Err(e) = auth_repo::delete_passkey_login_challenge(pool, &challenge_id).await {
        tracing::error!("DB error: {:?}", e);
    }

    let auth_state: webauthn_rs::prelude::PasskeyAuthentication = match auth_json {
        Some(json) => match serde_json::from_str(&json) {
            Ok(s) => s,
            Err(_) => return (StatusCode::BAD_REQUEST, "認証セッションが無効です").into_response(),
        },
        None => return (StatusCode::BAD_REQUEST, "認証セッションが見つかりません").into_response(),
    };

    let email = auth_repo::find_email_by_user_id(pool, user_id)
        .await
        .ok()
        .flatten();

    // ユーザー単位の試行の予約(確認コードと合算。DEMO-000153 S-1)。ロック中は、確認せずに断る
    match mfa_attempt_repo::reserve_attempt(
        pool,
        user_id,
        MFA_MAX_ATTEMPTS,
        chrono::Duration::minutes(MFA_LOCK_WINDOW_MINUTES),
    )
    .await
    {
        Ok(r) if r.outcome == AttemptReservation::Locked => {
            tracing::warn!(user_id, "確認の試行が上限に達したため、一時的に止めています");
            return locked_response(r.retry_after_secs.unwrap_or(MFA_LOCK_WINDOW_MINUTES * 60));
        }
        Ok(_) => {}
        Err(e) => {
            tracing::error!("確認の試行の予約に失敗: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "ログイン処理に失敗しました").into_response();
        }
    }

    match webauthn_service::finish_authentication(&webauthn, &auth_state, &credential) {
        Ok(auth_result) => {
            if let Err(e) = mfa_attempt_repo::reset_attempts(pool, user_id).await {
                tracing::error!("ユーザー単位の試行の数の削除に失敗: {:?}", e);
            }
            if let Err(e) = state.passkey_attempt_store.reset(&mfa_key).await {
                tracing::error!("パスキーの試行の数のリセットに失敗: {:?}", e);
            }
            // パスキーの counter を更新（リプレイ攻撃防止）
            let _ = update_passkey_counter(pool, user_id, &auth_result).await;

            auth_repo::insert_auth_event(
                pool,
                "mfa_success",
                Some(user_id),
                email.as_deref(),
                Some(&ip),
                "passkey",
            )
            .await;

            // MFA検証成功 → access + refresh トークンペアを発行し、
            // mfa_pending・challenge双方のCookieを削除
            let pair = match auth_jwt::issue_and_encode_token_pair(user_id, &role, &state.secret_key)
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::error!("トークンペア発行に失敗: user_id={} {:?}", user_id, e);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "ログイン処理に失敗しました",
                    )
                        .into_response();
                }
            };
            let jar = cookie_util::add_staff_session_cookies(jar, pair.access, pair.refresh);
            let clear_pending = cookie_util::build_auth_cookie(
                "sophia_mfa_pending",
                String::new(),
                Duration::seconds(0),
            );
            let clear_challenge = cookie_util::build_auth_cookie(
                "mfa_passkey_challenge",
                String::new(),
                Duration::seconds(0),
            );

            (
                jar.remove(clear_pending)
                    .remove(clear_challenge),
                (StatusCode::OK, "認証成功"),
            )
                .into_response()
        }
        Err(e) => {
            auth_repo::insert_auth_event(
                pool,
                "mfa_fail",
                Some(user_id),
                email.as_deref(),
                Some(&ip),
                "passkey",
            )
            .await;
            (StatusCode::UNAUTHORIZED, e.to_string()).into_response()
        }
    }
}

/// パスキーログインの試行の上限で断るときの応答(429 + Retry-After。パスキーログインの応答の形に合わせる)
fn passkey_login_locked_response(retry_after_secs: u64) -> axum::response::Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
        Json(serde_json::json!({
            "status": "error",
            "error": LOCKED_MSG,
            "retry_after_secs": retry_after_secs
        })),
    )
        .into_response()
}

/// IP + ユーザーの試行回数の上限で断るときの応答(429 + Retry-After。TOTP・パスキー MFA 共通の形)
fn mfa_store_locked_response(retry_after_secs: u64) -> axum::response::Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
        axum::Json(serde_json::json!({
            "success": false,
            "error": LOCKED_MSG,
            "retry_after_secs": retry_after_secs
        })),
    )
        .into_response()
}

/// ユーザー単位のロックで断るときの応答(429 + Retry-After)
fn locked_response(retry_after_secs: i64) -> axum::response::Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
        axum::Json(serde_json::json!({
            "success": false,
            "error": LOCKED_MSG,
            "retry_after_secs": retry_after_secs
        })),
    )
        .into_response()
}

/// `sophia_mfa_pending` Cookie を検証し user_id を取り出す（MFAパスキーフロー共通処理）。
/// 失敗時は呼び出し元がそのまま返せるレスポンスを返す。
#[allow(clippy::result_large_err)] // Errは呼び出し元にそのまま返すaxum::response::Responseのため許容
fn mfa_pending_user_id(jar: &CookieJar, secret: &str) -> Result<i64, axum::response::Response> {
    mfa_pending_user_id_and_role(jar, secret).map(|(id, _role)| id)
}

/// `sophia_mfa_pending` Cookie を検証し (user_id, role) を取り出す。
#[allow(clippy::result_large_err)]
fn mfa_pending_user_id_and_role(
    jar: &CookieJar,
    secret: &str,
) -> Result<(i64, String), axum::response::Response> {
    let invalid = || (StatusCode::BAD_REQUEST, "セッションが無効です").into_response();

    let pending_token = jar
        .get("sophia_mfa_pending")
        .map(|c| c.value().to_string())
        .ok_or_else(invalid)?;
    let (claims, extra) =
        auth_jwt::decode_sophia_claims(&pending_token, secret).map_err(|_| invalid())?;
    if extra.token_type != auth_jwt::TOKEN_TYPE_MFA_PENDING {
        return Err(invalid());
    }
    let user_id: i64 = claims.sub.parse().map_err(|_| invalid())?;
    Ok((user_id, extra.role))
}

// ── パスキーログイン（認証不要）──

/// POST /passkey/login/begin — パスキーログイン開始（未認証・JSON API）
///
/// 全ユーザーのパスキーからWebAuthn認証チャレンジを生成。
/// auth_state はCookie（署名付き）に保存する。
pub async fn passkey_login_begin(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: ClientPeer,
    jar: CookieJar,
) -> impl IntoResponse {
    // 開始の回数を IP 単位で数える(記録してから数える)。乱発されやすいので、上限は緩め(15 分に 30 回)。
    // 接続元が解釈できないときは数えない(全員のロックを避ける。人が決める点は設計書に記載)
    let ip = client_ip(peer.0, &headers);
    if let Some(key) = login_attempt_repo::passkey_ip_key(&ip, "begin") {
        match state.passkey_begin_attempt_store.reserve(&key).await {
            Ok(None) => {}
            Ok(Some(retry_after_secs)) => return mfa_store_locked_response(retry_after_secs),
            Err(e) => {
                tracing::error!("パスキーログインの開始の試行の予約に失敗: {:?}", e);
                return (StatusCode::INTERNAL_SERVER_ERROR, "ログイン処理に失敗しました").into_response();
            }
        }
    }

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    // 全ユーザーのパスキーを取得
    let rows = auth_repo::list_all_passkeys_with_user(&state.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("auth: fetch_all failed: {:?}", e);
            vec![]
        });

    let passkeys: Vec<webauthn_rs::prelude::Passkey> = rows
        .iter()
        .filter_map(|(_, json)| webauthn_service::passkey_from_json(json).ok())
        .collect();

    if passkeys.is_empty() {
        return (StatusCode::BAD_REQUEST, "登録済みパスキーがありません").into_response();
    }

    match webauthn_service::start_authentication(&webauthn, &passkeys) {
        Ok((rcr, auth_state)) => {
            // auth_state をDBの一時テーブルに保存（チャレンジID付き）
            let challenge_id = uuid::Uuid::new_v4().to_string();
            let auth_json = serde_json::to_string(&auth_state).unwrap_or_default();

            // s_passkey_login_challenge に保存（5分で期限切れ）
            if let Err(e) =
                auth_repo::insert_passkey_login_challenge(&state.pool, &challenge_id, &auth_json)
                    .await
            {
                tracing::error!("DB error: {:?}", e);
            }

            // チャレンジIDをCookieで返す
            let cookie = cookie_util::build_auth_cookie(
                "passkey_challenge",
                challenge_id,
                Duration::minutes(5),
            );

            (jar.add(cookie), Json(rcr)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

/// POST /passkey/login/complete — パスキーログイン完了（未認証・JSON API）
///
/// ブラウザからのアサーション結果を検証し、成功すればセッション作成。
/// userHandle からユーザーを特定する。
pub async fn passkey_login_complete(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    peer: ClientPeer,
    jar: CookieJar,
    Json(credential): Json<webauthn_rs::prelude::PublicKeyCredential>,
) -> impl IntoResponse {
    // 失敗(署名の検証失敗・不明な資格情報など)を IP 単位で数える(DEMO-000233 S2-D / 項目6-1)。
    // 記録してから数え、上限を超えていれば検証せずに断る。成功のとき、同じ IP の失敗を消す(下の成功の分岐)
    let ip = client_ip(peer.0, &headers);
    let login_key = login_attempt_repo::passkey_ip_key(&ip, "login");
    if let Some(key) = &login_key {
        match state.passkey_attempt_store.reserve(key).await {
            Ok(None) => {}
            Ok(Some(retry_after_secs)) => return passkey_login_locked_response(retry_after_secs),
            Err(e) => {
                tracing::error!("パスキーログインの試行の予約に失敗: {:?}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"status": "error", "error": "ログイン処理に失敗しました"})),
                )
                    .into_response();
            }
        }
    }

    let webauthn = match webauthn_service::create_webauthn_from_headers(&headers) {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"status": "error", "error": e.to_string()})),
            )
                .into_response()
        }
    };

    // CookieからチャレンジID取得
    let challenge_id =
        match jar.get("passkey_challenge") {
            Some(c) => c.value().to_string(),
            None => return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"status": "error", "error": "チャレンジが見つかりません"})),
            )
                .into_response(),
        };

    // DBからauth_state取得
    let auth_json = auth_repo::find_passkey_login_challenge(&state.pool, &challenge_id)
        .await
        .ok()
        .flatten();

    // 使用済みチャレンジを削除
    if let Err(e) = auth_repo::delete_passkey_login_challenge(&state.pool, &challenge_id).await {
        tracing::error!("DB error: {:?}", e);
    }

    let auth_state: webauthn_rs::prelude::PasskeyAuthentication = match auth_json {
        Some(json) => {
            match serde_json::from_str(&json) {
                Ok(s) => s,
                Err(_) => return (
                    StatusCode::BAD_REQUEST,
                    Json(
                        serde_json::json!({"status": "error", "error": "認証セッションが無効です"}),
                    ),
                )
                    .into_response(),
            }
        }
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"status": "error", "error": "チャレンジが期限切れです"})),
            )
                .into_response()
        }
    };

    match webauthn_service::finish_authentication(&webauthn, &auth_state, &credential) {
        Ok(auth_result) => {
            // credential_id からユーザーを特定
            let cred_id_b64 = {
                use base64::engine::general_purpose::URL_SAFE_NO_PAD;
                use base64::Engine;
                URL_SAFE_NO_PAD.encode(auth_result.cred_id())
            };

            let user_row = auth_repo::find_user_by_credential_id(&state.pool, &cred_id_b64)
                .await
                .ok()
                .flatten();

            match user_row {
                Some((user_id, is_staff, _mfa_enabled)) => {
                    // ロール判定: api_login と同じロジックに統一（Admin > Employee > Partner > USER）。
                    // 旧実装はis_staffのみで判定しており、パスキーログインしたEMPLOYEE/PARTNER
                    // ユーザーが常にUSER扱いになっていた（P1-5）。
                    let role = if is_staff {
                        "ADMIN"
                    } else {
                        let profile =
                            auth_repo::find_user_profile_role_fields(&state.pool, user_id)
                                .await
                                .ok()
                                .flatten();
                        match profile {
                            Some((_, Some(_))) => "EMPLOYEE",
                            Some((Some(_), _)) => "PARTNER",
                            _ => "USER",
                        }
                    };

                    // パスキーログインなのでMFA検証済み扱い（Step3以前からの既存方針を維持。
                    // 6-3でユーザー確認済み: mfa_enabledの値に関わらずパスキー自体をMFA代替とみなす）。
                    // access + refresh トークンペアを発行する。
                    let pair = match auth_jwt::issue_and_encode_token_pair(user_id, &role, &state.secret_key)
                    {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!(
                                "パスキーログイン: トークンペア発行に失敗: user_id={} {:?}",
                                user_id,
                                e
                            );
                            return (StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({"status": "error", "error": "ログイン処理に失敗しました"}))).into_response();
                        }
                    };

                    // パスキーのcounterを更新
                    let _ = update_passkey_counter(&state.pool, user_id, &auth_result).await;

                    // 成功: 同じ IP の失敗を消す(別の IP の失敗は残す)
                    if let Some(key) = &login_key {
                        if let Err(e) = state.passkey_attempt_store.reset(key).await {
                            tracing::error!("パスキーログインの成功後の試行の無効化に失敗: {:?}", e);
                        }
                    }

                    let jar = cookie_util::add_staff_session_cookies(jar, pair.access, pair.refresh);

                    // passkey_challenge Cookieを削除
                    let remove_cookie = cookie_util::build_auth_cookie(
                        "passkey_challenge",
                        String::new(),
                        Duration::seconds(0),
                    );

                    (
                        jar.remove(remove_cookie),
                        Json(serde_json::json!({"success": true})),
                    )
                        .into_response()
                }
                None => (
                    StatusCode::UNAUTHORIZED,
                    Json(
                        serde_json::json!({"status": "error", "error": "ユーザーが見つかりません"}),
                    ),
                )
                    .into_response(),
            }
        }
        Err(e) => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"status": "error", "error": e.to_string()})),
        )
            .into_response(),
    }
}

// ── ヘルパー ──

/// 認証成功後にパスキーの counter を更新する
async fn update_passkey_counter(
    pool: &PgPool,
    user_id: i64,
    auth_result: &webauthn_rs::prelude::AuthenticationResult,
) -> anyhow::Result<()> {
    // 該当パスキーを取得して counter を更新
    let cred_id_b64 = {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        URL_SAFE_NO_PAD.encode(auth_result.cred_id())
    };

    let row =
        auth_repo::find_webauthn_credential_by_user_and_cred(pool, user_id, &cred_id_b64).await?;

    if let Some((id, json)) = row {
        if let Ok(mut passkey) = webauthn_service::passkey_from_json(&json) {
            passkey.update_credential(auth_result);
            if let Ok(updated_json) = webauthn_service::passkey_to_json(&passkey) {
                if let Err(e) =
                    auth_repo::update_webauthn_credential_passkey_json(pool, id, &updated_json)
                        .await
                {
                    tracing::error!("DB error: {:?}", e);
                }
            }
        }
    }

    Ok(())
}
