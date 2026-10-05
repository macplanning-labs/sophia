-- P5-1e: 認証イベント監査ログ
CREATE TABLE IF NOT EXISTS h_auth_event (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(32) NOT NULL,
    user_id BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    email VARCHAR(255),
    ip VARCHAR(64),
    detail TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_auth_event_created ON h_auth_event (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_auth_event_type ON h_auth_event (event_type);
CREATE INDEX IF NOT EXISTS idx_auth_event_user ON h_auth_event (user_id);
