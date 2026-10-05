-- ============================================================
-- 020_deadline_settings.sql — 発注契約の締め日設定
-- ============================================================
-- 旧EDI_MP(Django版) OrderBasicInfo 相当の5項目を追加し、
-- パイプラインダッシュボード(home.rs)の期限計算を契約ごとに
-- 設定可能にする。受注側は既存の m_client_contract.report_deadline_days_before
-- (001_init.sql) をそのまま使うためスキーマ変更なし。

ALTER TABLE m_partner_contract
    ADD COLUMN IF NOT EXISTS order_create_deadline_day INTEGER NOT NULL DEFAULT 15,
    ADD COLUMN IF NOT EXISTS order_approve_deadline_days_before INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS report_upload_deadline_days_before INTEGER NOT NULL DEFAULT 2,
    ADD COLUMN IF NOT EXISTS invoice_create_deadline_day INTEGER NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS invoice_approve_deadline_day INTEGER NOT NULL DEFAULT 10;

COMMENT ON COLUMN m_partner_contract.order_create_deadline_day IS '発注書作成期限: 前月N日（月末クリップ）';
COMMENT ON COLUMN m_partner_contract.order_approve_deadline_days_before IS '承諾期限: 前月末からN営業日前';
COMMENT ON COLUMN m_partner_contract.report_upload_deadline_days_before IS '報告提出期限: 当月末からN営業日前';
COMMENT ON COLUMN m_partner_contract.invoice_create_deadline_day IS '請求書作成期限: 翌月N日';
COMMENT ON COLUMN m_partner_contract.invoice_approve_deadline_day IS '請求承諾期限: 翌月N日';
