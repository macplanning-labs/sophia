-- ============================================================
-- 014_passkey_login_challenge.sql — パスキーログインチャレンジテーブル
-- ============================================================

CREATE TABLE IF NOT EXISTS s_passkey_login_challenge (
    id           SERIAL PRIMARY KEY,
    challenge_id VARCHAR(255) NOT NULL UNIQUE,
    auth_state   TEXT NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
