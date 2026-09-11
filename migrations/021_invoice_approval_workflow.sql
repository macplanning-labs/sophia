-- ============================================================
-- 021_invoice_approval_workflow.sql — 請求書承認・送信ワークフロー
-- ============================================================
-- 概要設計書「請求書承認・送信ワークフロー」節（2026-07-10合意）の実装。
-- 既存のt_billing_invoice.statusをそのまま拡張し、PENDING_APPROVAL/APPROVED/SENT
-- という値を追加で使う（新しいstatusカラムは作らない）。
-- create_invoices_by_clientでのINSERT時のデフォルトは 'CONFIRMED' から
-- 'PENDING_APPROVAL' に変更する（アプリケーションコード側で対応）。

ALTER TABLE t_billing_invoice
    ADD COLUMN IF NOT EXISTS approved_by_id BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS approved_at    TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS sent_at        TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS sent_subject   TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS sent_body      TEXT NOT NULL DEFAULT '';

COMMENT ON COLUMN t_billing_invoice.approved_by_id IS '承認したAdminユーザー（s_user.id）';
COMMENT ON COLUMN t_billing_invoice.approved_at IS '承認日時';
COMMENT ON COLUMN t_billing_invoice.sent_at IS 'クライアントへのメール送信日時';
COMMENT ON COLUMN t_billing_invoice.sent_subject IS '実際に送信したメール件名（編集後の内容、監査用）';
COMMENT ON COLUMN t_billing_invoice.sent_body IS '実際に送信したメール本文（編集後の内容、監査用）';
