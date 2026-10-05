-- ============================================================
-- 062_billing_force_confirm.sql — 請求書の強制確定の履歴(UI刷新 2-4 / DEMO-000136)
-- ============================================================
-- 勤務表が揃っていない請求書を、理由つきで確定したときの記録。
-- 誰が・いつ・なぜ・どの契約を・どう扱ったか(NEXT_MONTH=翌月回し / SECOND_INVOICE=当月2通目で後日発行)。
-- 追加のみ。請求書を削除したら、その履歴も一緒に消える(CASCADE)。

CREATE TABLE IF NOT EXISTS t_billing_force_confirm (
    id                 BIGSERIAL PRIMARY KEY,
    invoice_id         BIGINT      NOT NULL REFERENCES t_billing_invoice(id) ON DELETE CASCADE,
    client_contract_id BIGINT      NOT NULL,
    action             VARCHAR(14) NOT NULL CHECK (action IN ('NEXT_MONTH', 'SECOND_INVOICE')),
    reason             TEXT        NOT NULL CHECK (length(btrim(reason)) > 0),
    created_by_id      BIGINT      NOT NULL REFERENCES s_user(id),
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_billing_force_confirm_invoice ON t_billing_force_confirm (invoice_id);

COMMENT ON TABLE t_billing_force_confirm IS '勤務表が揃わないまま請求書を確定した履歴(理由必須)';
