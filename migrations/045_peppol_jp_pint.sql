-- 045: Peppol / JP PINT 対応（フェーズA: スキーマ拡張）

-- 明細への税率列追加（既定10%。既存データは全て10%扱いのため後方互換）
ALTER TABLE t_billing_invoice_item ADD COLUMN IF NOT EXISTS tax_rate NUMERIC(5,2) NOT NULL DEFAULT 10.00;
ALTER TABLE t_payment_notice_item  ADD COLUMN IF NOT EXISTS tax_rate NUMERIC(5,2) NOT NULL DEFAULT 10.00;

-- Peppol参加者ID（取引先ごと。送信先ルーティングに使用）
ALTER TABLE m_client  ADD COLUMN IF NOT EXISTS peppol_participant_id VARCHAR(64) NOT NULL DEFAULT '';
ALTER TABLE m_partner ADD COLUMN IF NOT EXISTS peppol_participant_id VARCHAR(64) NOT NULL DEFAULT '';

-- Peppol送受信ログ（監査証跡 + 突合状態を1レコードで管理）
CREATE TABLE IF NOT EXISTS t_peppol_transmission (
    id                BIGSERIAL PRIMARY KEY,
    direction         VARCHAR(10) NOT NULL,             -- OUTBOUND / INBOUND
    document_type     VARCHAR(20) NOT NULL,              -- INVOICE / SELF_BILLING
    related_table     VARCHAR(30) NOT NULL DEFAULT '',   -- 't_billing_invoice' / 't_payment_notice'
    related_id        VARCHAR(50) NOT NULL DEFAULT '',   -- invoice.id or notice_id（型が異なるためVARCHAR統一）
    peppol_message_id VARCHAR(100) NOT NULL DEFAULT '',
    participant_id    VARCHAR(64) NOT NULL DEFAULT '',
    status            VARCHAR(20) NOT NULL DEFAULT 'PENDING', -- PENDING/SENT/ACKED/FAILED/RECEIVED/MATCHED/UNMATCHED
    request_payload   JSONB,
    response_payload  JSONB,
    error_message     TEXT NOT NULL DEFAULT '',
    occurred_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_peppol_tx_related ON t_peppol_transmission(related_table, related_id);
CREATE INDEX IF NOT EXISTS idx_peppol_tx_status ON t_peppol_transmission(status);
