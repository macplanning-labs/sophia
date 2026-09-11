/// infrastructure/repositories/invite_repo.rs — パートナー招待(s_partner_invitation) CRUD

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

/// 招待情報（トークン検証・受諾フロー用）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InvitationRow {
    pub id: i32,
    pub partner_id: String,
    pub email: String,
    pub display_name: String,
    pub is_used: bool,
    pub expires_at: DateTime<Utc>,
}

/// 招待情報（未使用のもの限定・パスワード登録フロー用）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UnusedInvitation {
    pub id: i32,
    pub partner_id: String,
    pub email: String,
    pub display_name: String,
}

/// 招待情報 + WebAuthn登録state（パスキー登録完了フロー用）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UnusedInvitationWithRegState {
    pub id: i32,
    pub partner_id: String,
    pub email: String,
    pub display_name: String,
    pub webauthn_reg_state: Option<String>,
}

/// 招待トークンを発行する
pub async fn insert_invitation(
    pool: &PgPool,
    partner_id: &str,
    token: Uuid,
    email: &str,
    display_name: &str,
    expires_at: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_partner_invitation (partner_id, token, email, display_name, expires_at) VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(partner_id)
    .bind(token)
    .bind(email)
    .bind(display_name)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// トークンから招待情報を取得する（使用済み含む）
pub async fn find_invitation_by_token(pool: &PgPool, token: Uuid) -> Result<Option<InvitationRow>> {
    let row = sqlx::query_as::<_, InvitationRow>(
        "SELECT id, partner_id, email, display_name, is_used, expires_at FROM s_partner_invitation WHERE token = $1"
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// トークンから未使用の招待情報を取得する
pub async fn find_unused_invitation(pool: &PgPool, token: Uuid) -> Result<Option<UnusedInvitation>> {
    let row = sqlx::query_as::<_, UnusedInvitation>(
        "SELECT id, partner_id, email, display_name FROM s_partner_invitation WHERE token = $1 AND is_used = false"
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// トークンから未使用の招待情報 + WebAuthn登録stateを取得する
pub async fn find_unused_invitation_with_reg_state(pool: &PgPool, token: Uuid) -> Result<Option<UnusedInvitationWithRegState>> {
    let row = sqlx::query_as::<_, UnusedInvitationWithRegState>(
        "SELECT id, partner_id, email, display_name, webauthn_reg_state FROM s_partner_invitation WHERE token = $1 AND is_used = false"
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 招待を使用済みにする
pub async fn mark_invitation_used(pool: &PgPool, id: i32) -> Result<()> {
    sqlx::query("UPDATE s_partner_invitation SET is_used = true WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 招待にWebAuthn登録用stateを一時保存する（セッションが無いため招待テーブルを流用）
pub async fn update_invitation_reg_state(pool: &PgPool, invitation_id: i32, reg_json: &str) -> Result<()> {
    sqlx::query("UPDATE s_partner_invitation SET webauthn_reg_state = $1 WHERE id = $2")
        .bind(reg_json)
        .bind(invitation_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// メールアドレス+パートナーIDで有効なエンジニアを検索する
pub async fn find_active_engineer_id(pool: &PgPool, email: &str, partner_id: &str) -> Result<Option<i64>> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_engineer WHERE email = $1 AND partner_id = $2 AND is_active = true"
    )
    .bind(email)
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// エンジニア用セッションを作成する（マジックリンクログイン）
pub async fn insert_engineer_session(pool: &PgPool, session_id: &str, engineer_id: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO s_session (session_id, engineer_id, role, expires_at) VALUES ($1, $2, 'ENGINEER', NOW() + INTERVAL '30 days')"
    )
    .bind(session_id)
    .bind(engineer_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// パートナーユーザーをパスワード方式で作成する（招待受諾。旧フロー互換）
///
/// トランザクション内で ①ユーザー作成 ②プロフィール作成 ③招待を使用済みに更新 を行う。
/// ②③の失敗はログのみ（ユーザー作成自体は継続・コミットする＝既存挙動を踏襲）。
pub async fn create_partner_user_with_password(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
    display_name: &str,
    partner_id: &str,
    invitation_id: i32,
) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO s_user (email, password, username, is_active, is_staff, mfa_enabled) VALUES ($1, $2, $3, true, false, false) RETURNING id"
    )
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .fetch_one(&mut *tx)
    .await?;

    if let Err(e) = sqlx::query(
        "INSERT INTO s_user_profile (user_id, partner_id, is_first_login) VALUES ($1, $2, false)"
    )
    .bind(user_id)
    .bind(partner_id)
    .execute(&mut *tx)
    .await {
        tracing::error!("プロフィール作成エラー: {}", e);
    }

    if let Err(e) = sqlx::query("UPDATE s_partner_invitation SET is_used = true WHERE id = $1")
        .bind(invitation_id)
        .execute(&mut *tx)
        .await {
        tracing::error!("招待更新エラー: {}", e);
    }

    tx.commit().await?;
    Ok(user_id)
}

/// パートナーユーザーをパスキー方式で作成する（招待受諾）
///
/// トランザクション内で ①ユーザー作成 ②プロフィール作成 ③パスキー保存 ④招待を使用済みに更新 を行う。
/// ②③④の失敗はログのみ（ユーザー作成自体は継続・コミットする＝既存挙動を踏襲）。
pub async fn create_partner_user_with_passkey(
    pool: &PgPool,
    email: &str,
    display_name: &str,
    partner_id: &str,
    invitation_id: i32,
    credential_id: &str,
    passkey_json: &serde_json::Value,
) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let user_id: i64 = sqlx::query_scalar(
        "INSERT INTO s_user (email, password, username, is_active, is_staff, mfa_enabled) VALUES ($1, $2, $3, true, false, true) RETURNING id"
    )
    .bind(email)
    .bind("$2b$12$passkey-only-no-password-hash")
    .bind(display_name)
    .fetch_one(&mut *tx)
    .await?;

    if let Err(e) = sqlx::query(
        "INSERT INTO s_user_profile (user_id, partner_id, is_first_login) VALUES ($1, $2, false)"
    )
    .bind(user_id)
    .bind(partner_id)
    .execute(&mut *tx)
    .await { tracing::error!("DB error: {:?}", e); }

    if let Err(e) = sqlx::query(
        "INSERT INTO s_webauthn_credential (user_id, credential_id, passkey_json, name) VALUES ($1, $2, $3, $4)"
    )
    .bind(user_id)
    .bind(credential_id)
    .bind(passkey_json)
    .bind("パスキー")
    .execute(&mut *tx)
    .await { tracing::error!("DB error: {:?}", e); }

    if let Err(e) = sqlx::query("UPDATE s_partner_invitation SET is_used = true WHERE id = $1")
        .bind(invitation_id)
        .execute(&mut *tx)
        .await { tracing::error!("DB error: {:?}", e); }

    tx.commit().await?;
    Ok(user_id)
}
