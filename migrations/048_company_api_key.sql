-- 048_company_api_key.sql — 企業向けAPIキー認証テーブル

CREATE TABLE t_company_api_key (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    party_type      VARCHAR(10) NOT NULL,   -- 'CLIENT' 固定（Phase 1はクライアントのみ）
    party_id        VARCHAR(50) NOT NULL,   -- m_client.id を文字列化して格納
    key_prefix      VARCHAR(16) NOT NULL,   -- 'sk_live_' / 'sk_test_'
    api_key_hash    VARCHAR(64) NOT NULL UNIQUE,  -- SHA256(生キー) 無ソルト
    scope           VARCHAR(20) NOT NULL DEFAULT 'READ',  -- 'READ' / 'READ_WRITE'
    name            VARCHAR(64) NOT NULL DEFAULT '',
    is_active       BOOLEAN NOT NULL DEFAULT true,
    revoked_at      TIMESTAMPTZ,
    last_used_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_api_key_party ON t_company_api_key(party_type, party_id);

CREATE TABLE t_api_access_log (
    id              BIGSERIAL PRIMARY KEY,
    api_key_id      UUID NOT NULL REFERENCES t_company_api_key(id),
    action          VARCHAR(30) NOT NULL,   -- 'LIST_INVOICES' / 'GET_INVOICE' / 'ACCEPT_INVOICE' / 'POST_ORDER'
    related_table   VARCHAR(30) NOT NULL DEFAULT '',
    related_id      VARCHAR(50) NOT NULL DEFAULT '',
    http_status     SMALLINT NOT NULL,
    ip_address      VARCHAR(45) NOT NULL DEFAULT '',
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_api_access_log_key ON t_api_access_log(api_key_id, occurred_at);
