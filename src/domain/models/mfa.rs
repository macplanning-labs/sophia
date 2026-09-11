/// domain/models/mfa.rs — MFA関連エンティティ
///
/// TOTP デバイス、WebAuthn クレデンシャル、パートナー招待トークン。
/// Phase 1: 認証基盤 + MFA の一部。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ── TOTP デバイス ──

/// TOTP ワンタイムパスワードのデバイス情報
/// 秘密鍵は AES-GCM で暗号化してDBに保存する
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct TotpDevice {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub secret_encrypted: String,
    pub nonce: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

// ── WebAuthn クレデンシャル ──

/// WebAuthn パスキーの保存情報
/// passkey_json には webauthn-rs の Passkey 型を JSON シリアライズして格納
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WebAuthnCredential {
    pub id: i64,
    pub user_id: i64,
    pub credential_id: String,
    pub passkey_json: serde_json::Value,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

// ── パートナー招待 ──

/// パートナー招待トークン
/// 管理者がパートナーを招待 → メールでトークンURL送信 → パスキー登録
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PartnerInvitation {
    pub id: i64,
    pub partner_id: String,
    pub token: Uuid,
    pub email: String,
    pub display_name: String,
    pub is_used: bool,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

// ── フォーム型 ──

/// TOTP 検証フォーム（ログイン時の6桁コード入力）
#[derive(Debug, Deserialize)]
pub struct TotpVerifyForm {
    pub code: String,
}

/// パートナー招待フォーム
#[derive(Debug, Deserialize)]
pub struct InvitePartnerForm {
    pub partner_id: String,
    pub email: String,
    pub display_name: String,
}

/// 招待受諾フォーム（パスキー登録完了後）
#[derive(Debug, Deserialize)]
pub struct InviteAcceptForm {
    pub display_name: String,
}

/// パスキーに名前をつけるフォーム
#[derive(Debug, Deserialize)]
pub struct PasskeyNameForm {
    pub name: String,
}
