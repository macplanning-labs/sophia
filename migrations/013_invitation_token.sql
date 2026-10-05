-- ============================================================
-- 013_invitation_token.sql — エンジニア招待トークンテーブル
-- ============================================================

CREATE TABLE IF NOT EXISTS s_invitation_token (
    token       TEXT PRIMARY KEY,
    engineer_id BIGINT NOT NULL REFERENCES m_engineer(id),
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
