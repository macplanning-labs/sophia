-- DEMO-000170: GAS発Webhookの署名鍵を Sophia が自動で用意して保存する。
-- 鍵は SECRET_KEY による AES-GCM で暗号化して保存する（平文は置かない）。単一行運用。
CREATE TABLE IF NOT EXISTS s_webhook_secret (
    id                SMALLINT PRIMARY KEY DEFAULT 1 CHECK (id = 1),
    secret_enc        TEXT NOT NULL,
    secret_nonce      TEXT NOT NULL,
    prev_secret_enc   TEXT,
    prev_secret_nonce TEXT,
    prev_expires_at   TIMESTAMPTZ,
    rotated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
