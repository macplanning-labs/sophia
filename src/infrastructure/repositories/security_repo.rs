/// infrastructure/repositories/security_repo.rs — MFA設定(s_totp_device/s_webauthn_credential/s_session/s_user) CRUD

use anyhow::Result;
use sqlx::PgPool;

/// パスキー一覧（SPA用JSON API向け：id, name, created_at）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PasskeySummary {
    pub id: i32,
    pub name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// ユーザーのTOTPデバイスを全て無効化する
pub async fn deactivate_totp_devices(pool: &PgPool, user_id: i64) -> Result<()> {
    sqlx::query("UPDATE s_totp_device SET is_active = false WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ユーザーのパスキー登録数を取得する
pub async fn count_passkeys(pool: &PgPool, user_id: i64) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM s_webauthn_credential WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// ユーザーの有効なTOTPデバイス数を取得する
pub async fn count_active_totp_devices(pool: &PgPool, user_id: i64) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM s_totp_device WHERE user_id = $1 AND is_active = true"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// TOTPデバイスを新規登録する（セットアップ完了時、暗号化済みの秘密鍵を保存）
pub async fn create_totp_device(
    pool: &PgPool,
    user_id: i64,
    secret_encrypted: &str,
    nonce: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_totp_device (user_id, secret_encrypted, nonce) VALUES ($1, $2, $3)"
    )
    .bind(user_id)
    .bind(secret_encrypted)
    .bind(nonce)
    .execute(pool)
    .await?;
    Ok(())
}

/// ユーザーのmfa_enabledフラグを更新する
pub async fn set_mfa_enabled(pool: &PgPool, user_id: i64, enabled: bool) -> Result<()> {
    sqlx::query("UPDATE s_user SET mfa_enabled = $1 WHERE id = $2")
        .bind(enabled)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ユーザーの最新の有効セッションIDを取得する
pub async fn find_latest_session_id(pool: &PgPool, user_id: i64) -> Result<Option<String>> {
    let session_id: Option<String> = sqlx::query_scalar(
        "SELECT session_id FROM s_session WHERE user_id = $1 AND expires_at > NOW() ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(session_id)
}

/// パスキー（WebAuthn認証情報）を登録する
pub async fn insert_webauthn_credential(
    pool: &PgPool,
    user_id: i64,
    credential_id: &str,
    passkey_json: &serde_json::Value,
    name: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_webauthn_credential (user_id, credential_id, passkey_json, name) VALUES ($1, $2, $3, $4)"
    )
    .bind(user_id)
    .bind(credential_id)
    .bind(passkey_json)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(())
}

/// パスキーを削除する（本人のもののみ）
pub async fn delete_webauthn_credential(pool: &PgPool, id: i64, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM s_webauthn_credential WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// ユーザーの全パスキーのpasskey_json一覧を取得する
pub async fn list_passkey_jsons(pool: &PgPool, user_id: i64) -> Result<Vec<serde_json::Value>> {
    let rows: Vec<(serde_json::Value,)> = sqlx::query_as(
        "SELECT passkey_json FROM s_webauthn_credential WHERE user_id = $1"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(json,)| json).collect())
}

/// ユーザーのパスキー概要一覧（SPA用JSON API向け）を取得する
pub async fn list_passkey_summaries(pool: &PgPool, user_id: i64) -> Result<Vec<PasskeySummary>> {
    let rows = sqlx::query_as::<_, PasskeySummary>(
        "SELECT id, name, created_at FROM s_webauthn_credential WHERE user_id = $1 ORDER BY created_at"
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
