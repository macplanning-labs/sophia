/// infrastructure/repositories/auth_repo.rs — 認証(s_user/s_session/s_totp_device/s_passkey_login_challenge/s_engineer_login_token) CRUD
///
/// ## Passkey チャレンジ管理の方針
/// ログイン・MFA・登録などの Passkey 中間状態は **DB テーブル + Cookie に challenge_id のみ**で管理。
/// MemoryStore / プロセス内ストアは採用しない。状態本体は DB に JSON 保存（webauthn-rs の
/// `danger-allow-state-serialisation` を利用）し、Cookie には challenge_id だけを載せる。
/// 詳細は `docs/spec/アプリ方式設計書.md` の認証節を参照。
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

/// セッションからuser_id・emailを取得する（有効期限内のもののみ）
pub async fn find_session_user_email(
    pool: &PgPool,
    session_id: &str,
) -> Result<Option<(i64, String)>> {
    let row = sqlx::query_as(
        "SELECT s.user_id, u.email FROM s_session s JOIN s_user u ON u.id = s.user_id WHERE s.session_id = $1 AND s.expires_at > NOW()"
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// user_idからemailを取得する（JWT（sophia_mfa_pending）のsubからユーザーを引く用途。
/// Step3: MFA検証がsophia_session経由からJWT経由に変わったことに伴い新設）
pub async fn find_email_by_user_id(pool: &PgPool, user_id: i64) -> Result<Option<String>> {
    let email: Option<String> = sqlx::query_scalar("SELECT email FROM s_user WHERE id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(email)
}

/// ユーザーの有効なTOTP secret（暗号化済み）を取得する
pub async fn find_active_totp_secret(
    pool: &PgPool,
    user_id: i64,
) -> Result<Option<(String, String)>> {
    let row = sqlx::query_as(
        "SELECT secret_encrypted, nonce FROM s_totp_device WHERE user_id = $1 AND is_active = true ORDER BY id DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 全ユーザーのパスキー（user_id, passkey_json）一覧を取得する（パスキーログイン用）
/// user_id は s_webauthn_credential.user_id (INTEGER) と型を合わせて i32 で受ける
pub async fn list_all_passkeys_with_user(pool: &PgPool) -> Result<Vec<(i32, serde_json::Value)>> {
    let rows = sqlx::query_as("SELECT user_id, passkey_json FROM s_webauthn_credential")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// パスキーログインチャレンジを保存する（5分で期限切れ）
pub async fn insert_passkey_login_challenge(
    pool: &PgPool,
    challenge_id: &str,
    auth_json: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_passkey_login_challenge (challenge_id, auth_state, expires_at) VALUES ($1, $2, NOW() + INTERVAL '5 minutes')"
    )
    .bind(challenge_id)
    .bind(auth_json)
    .execute(pool)
    .await?;
    Ok(())
}

/// パスキーログインチャレンジのauth_stateを取得する（有効期限内のもののみ）
pub async fn find_passkey_login_challenge(
    pool: &PgPool,
    challenge_id: &str,
) -> Result<Option<String>> {
    let auth_json: Option<String> = sqlx::query_scalar(
        "SELECT auth_state FROM s_passkey_login_challenge WHERE challenge_id = $1 AND expires_at > NOW()"
    )
    .bind(challenge_id)
    .fetch_optional(pool)
    .await?;
    Ok(auth_json)
}

/// パスキーログインチャレンジを削除する（使用済み）
pub async fn delete_passkey_login_challenge(pool: &PgPool, challenge_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM s_passkey_login_challenge WHERE challenge_id = $1")
        .bind(challenge_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// WebAuthn credential_id からログイン可能なユーザー（id, is_staff, mfa_enabled）を取得する
pub async fn find_user_by_credential_id(
    pool: &PgPool,
    cred_id_b64: &str,
) -> Result<Option<(i64, bool, bool)>> {
    let row = sqlx::query_as(
        "SELECT u.id, u.is_staff, u.mfa_enabled FROM s_user u \
         JOIN s_webauthn_credential c ON c.user_id = u.id \
         WHERE c.credential_id = $1 AND u.is_active = true",
    )
    .bind(cred_id_b64)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// セッションを削除する（ログアウト）
pub async fn delete_session(pool: &PgPool, session_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM s_session WHERE session_id = $1")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ユーザー・credential_id指定でパスキー（id, passkey_json）を取得する（counter更新用）
/// id は s_webauthn_credential.id (SERIAL/INTEGER) と型を合わせて i32 で受ける
pub async fn find_webauthn_credential_by_user_and_cred(
    pool: &PgPool,
    user_id: i64,
    cred_id_b64: &str,
) -> Result<Option<(i32, serde_json::Value)>> {
    let row = sqlx::query_as(
        "SELECT id, passkey_json FROM s_webauthn_credential WHERE user_id = $1 AND credential_id = $2"
    )
    .bind(user_id)
    .bind(cred_id_b64)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パスキーのpasskey_jsonを更新する（counter更新、リプレイ攻撃防止）
pub async fn update_webauthn_credential_passkey_json(
    pool: &PgPool,
    id: i32,
    updated_json: &serde_json::Value,
) -> Result<()> {
    sqlx::query("UPDATE s_webauthn_credential SET passkey_json = $1 WHERE id = $2")
        .bind(updated_json)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ログイン用ユーザー情報（id, email, password_hash, is_staff, mfa_enabled）を取得する
pub async fn find_user_for_login(
    pool: &PgPool,
    email: &str,
) -> Result<Option<(i64, String, String, bool, bool)>> {
    let row = sqlx::query_as(
        "SELECT id, email, password, is_staff, mfa_enabled FROM s_user WHERE email = $1 AND is_active = true"
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// ユーザープロフィールのロール判定フィールド（partner_id, employee_id）を取得する
pub async fn find_user_profile_role_fields(
    pool: &PgPool,
    user_id: i64,
) -> Result<Option<(Option<String>, Option<i64>)>> {
    let row =
        sqlx::query_as("SELECT partner_id, employee_id FROM s_user_profile WHERE user_id = $1")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// メールアドレスから有効なエンジニアを検索する（マジックリンクログイン用）
pub async fn find_engineer_by_email(pool: &PgPool, email: &str) -> Result<Option<(i64, String)>> {
    let row =
        sqlx::query_as("SELECT id, name FROM m_engineer WHERE email = $1 AND is_active = true")
            .bind(email)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// エンジニア用ログイントークンを発行する（24時間有効）
pub async fn insert_engineer_login_token(
    pool: &PgPool,
    engineer_id: i64,
    token: uuid::Uuid,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_engineer_login_token (engineer_id, token, expires_at) VALUES ($1, $2, $3)",
    )
    .bind(engineer_id)
    .bind(token)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// エンジニア用ログイントークンが有効か確認する（消費はしない）
pub async fn engineer_login_token_valid(pool: &PgPool, token: uuid::Uuid) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM s_engineer_login_token WHERE token = $1 AND is_used = false AND expires_at > NOW())"
    )
    .bind(token)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// エンジニア用ログイントークンを取得する（有効なもののみ）
pub async fn find_valid_engineer_login_token(
    pool: &PgPool,
    token: uuid::Uuid,
) -> Result<Option<(i64, i64)>> {
    let row = sqlx::query_as(
        "SELECT id, engineer_id FROM s_engineer_login_token WHERE token = $1 AND is_used = false AND expires_at > NOW()"
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// エンジニア用ログイントークンを使用済みにする
pub async fn mark_engineer_login_token_used(pool: &PgPool, token_id: i64) -> Result<()> {
    sqlx::query("UPDATE s_engineer_login_token SET is_used = true WHERE id = $1")
        .bind(token_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 認証監査イベントを記録する（失敗しても呼び出し側は続行）
pub async fn insert_auth_event(
    pool: &PgPool,
    event_type: &str,
    user_id: Option<i64>,
    email: Option<&str>,
    ip: Option<&str>,
    detail: &str,
) {
    if let Err(e) = sqlx::query(
        "INSERT INTO h_auth_event (event_type, user_id, email, ip, detail) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(event_type)
    .bind(user_id)
    .bind(email)
    .bind(ip)
    .bind(detail)
    .execute(pool)
    .await
    {
        tracing::warn!("auth audit insert failed: {:?}", e);
    }
}

/// ユーザーの全セッションを削除する（パスワードリセット後など）
pub async fn delete_sessions_for_user(pool: &PgPool, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM s_session WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}
