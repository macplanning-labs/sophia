-- 004_mfa_tables.sql — MFA（TOTP + WebAuthn）関連テーブル
-- 
-- Phase 1: 認証基盤 + MFA
-- - s_totp_device: TOTP デバイス（暗号化秘密鍵）
-- - s_webauthn_credential: WebAuthn クレデンシャル（パスキー）
-- - s_partner_invitation: パートナー招待トークン
-- - s_session.mfa_verified: MFA 検証済みフラグ
-- - s_user.mfa_enabled: MFA 有効化フラグ

-- ===== s_user に MFA 有効フラグ追加 =====
ALTER TABLE s_user ADD COLUMN IF NOT EXISTS mfa_enabled BOOLEAN NOT NULL DEFAULT false;

-- ===== s_session に MFA 検証済みフラグ + WebAuthn 中間状態保存 =====
ALTER TABLE s_session ADD COLUMN IF NOT EXISTS mfa_verified BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE s_session ADD COLUMN IF NOT EXISTS webauthn_reg_state TEXT;
ALTER TABLE s_session ADD COLUMN IF NOT EXISTS webauthn_auth_state TEXT;

-- ===== TOTP デバイス =====
CREATE TABLE IF NOT EXISTS s_totp_device (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES s_user(id) ON DELETE CASCADE,
    name VARCHAR(128) NOT NULL DEFAULT 'Authenticator',
    secret_encrypted TEXT NOT NULL,          -- AES-GCM で暗号化された秘密鍵
    nonce TEXT NOT NULL,                     -- AES-GCM の nonce（base64）
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_totp_device_user_id ON s_totp_device(user_id);

-- ===== WebAuthn クレデンシャル（パスキー）=====
-- webauthn-rs の Passkey 型を JSON でシリアライズして保存する
CREATE TABLE IF NOT EXISTS s_webauthn_credential (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES s_user(id) ON DELETE CASCADE,
    credential_id TEXT NOT NULL UNIQUE,       -- base64url エンコード
    passkey_json JSONB NOT NULL,              -- webauthn-rs Passkey の JSON シリアライズ
    name VARCHAR(128) NOT NULL DEFAULT '',    -- ユーザーが付けた名前
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_webauthn_credential_user_id ON s_webauthn_credential(user_id);

-- ===== パートナー招待トークン =====
CREATE TABLE IF NOT EXISTS s_partner_invitation (
    id SERIAL PRIMARY KEY,
    partner_id VARCHAR(32) NOT NULL REFERENCES m_partner(partner_id) ON DELETE CASCADE,
    token UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL,
    display_name VARCHAR(128) NOT NULL DEFAULT '',
    is_used BOOLEAN NOT NULL DEFAULT false,
    expires_at TIMESTAMPTZ NOT NULL,
    webauthn_reg_state TEXT,                -- パスキー登録の中間状態（JSON）
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_partner_invitation_token ON s_partner_invitation(token);
