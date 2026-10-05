-- 008: 請求書自社発行 — 明細テーブル作成 + UNIQUE制約解除

-- 請求書明細テーブル（エンジニアごとの明細行）
CREATE TABLE IF NOT EXISTS t_billing_invoice_item (
    id              BIGSERIAL PRIMARY KEY,
    invoice_id      BIGINT NOT NULL REFERENCES t_billing_invoice(id) ON DELETE CASCADE,
    engineer_id     BIGINT NOT NULL REFERENCES m_engineer(id),
    description     VARCHAR(255) NOT NULL DEFAULT '',
    quantity        NUMERIC(6,2) NOT NULL DEFAULT 0,
    unit_price      INTEGER NOT NULL DEFAULT 0,
    amount          INTEGER NOT NULL DEFAULT 0,
    settlement_type VARCHAR(10) NOT NULL DEFAULT 'RANGE',
    lower_limit     NUMERIC(5,1) NOT NULL DEFAULT 140.0,
    upper_limit     NUMERIC(5,1) NOT NULL DEFAULT 180.0,
    deduction_rate  INTEGER NOT NULL DEFAULT 0,
    overtime_rate   INTEGER NOT NULL DEFAULT 0,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- UNIQUE制約を解除（同月同クライアントで複数請求書を発行可能に）
ALTER TABLE t_billing_invoice DROP CONSTRAINT IF EXISTS t_billing_invoice_client_id_target_month_key;
CREATE INDEX IF NOT EXISTS idx_billing_invoice_client_month ON t_billing_invoice(client_id, target_month);

-- 受注書への紐付けカラム追加
ALTER TABLE t_billing_invoice ADD COLUMN IF NOT EXISTS received_order_id BIGINT;
