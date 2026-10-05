-- ============================================================
-- 061_billing_unit.sql — 請求書の単位を取引先ごとに設定できるようにする(UI刷新 2-3 / DEMO-000135)
-- ============================================================
-- m_client.billing_unit:
--   'PROJECT' = 案件ごと(既定。現行どおり (client_id, project_id) で1通)
--   'CLIENT'  = 取引先まとめ(その月の全案件・全要員を1通)
-- t_billing_invoice.project_id / billing_unit:
--   新しく発行する請求書にだけ書く。既存の行は NULL / 'PROJECT' のまま(書き換えない)。
--
-- 二重発行を防ぐ一意索引は、ここでは張らない。現行は同じ取引先・案件・月でも、後から承認された要員の分を
-- 2通目として発行できる(invoice_issued は要員単位)ため、索引を張ると既存の運用を壊す。
-- 重複防止は、確定API(UI刷新 2-4)で「当月2通目」の扱いと一緒に設計する。

ALTER TABLE m_client
    ADD COLUMN IF NOT EXISTS billing_unit VARCHAR(8) NOT NULL DEFAULT 'PROJECT'
    CHECK (billing_unit IN ('PROJECT', 'CLIENT'));

ALTER TABLE t_billing_invoice
    ADD COLUMN IF NOT EXISTS project_id VARCHAR(32) NULL REFERENCES m_project(project_id);

ALTER TABLE t_billing_invoice
    ADD COLUMN IF NOT EXISTS billing_unit VARCHAR(8) NOT NULL DEFAULT 'PROJECT'
    CHECK (billing_unit IN ('PROJECT', 'CLIENT'));

COMMENT ON COLUMN m_client.billing_unit IS '請求書の単位: PROJECT=案件ごと(既定) / CLIENT=取引先まとめ';
COMMENT ON COLUMN t_billing_invoice.project_id IS '案件単位で発行した請求書の案件ID。旧い請求書・取引先まとめは NULL';
COMMENT ON COLUMN t_billing_invoice.billing_unit IS '発行時の請求単位。旧い請求書は PROJECT';
