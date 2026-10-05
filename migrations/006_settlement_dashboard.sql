-- 006_settlement_dashboard.sql
-- 月次売上・支払 確定ダッシュボード用のDB変更

-- ─── s_company_info: 設定値追加 ───
ALTER TABLE s_company_info
    ADD COLUMN IF NOT EXISTS notice_approval_threshold INTEGER DEFAULT 500000,
    ADD COLUMN IF NOT EXISTS token_expiry_days INTEGER DEFAULT 14;

COMMENT ON COLUMN s_company_info.notice_approval_threshold IS '支払通知書の上司承認基準金額（この金額以上で承認必須）';
COMMENT ON COLUMN s_company_info.token_expiry_days IS 'パートナートークンURLの有効期限（日数）';

-- ─── t_payment_notice: 上司承認フロー用カラム追加 ───
ALTER TABLE t_payment_notice
    ADD COLUMN IF NOT EXISTS approval_status VARCHAR(32) DEFAULT 'NONE',
    ADD COLUMN IF NOT EXISTS approval_requested_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS approval_requested_by_id BIGINT,
    ADD COLUMN IF NOT EXISTS approved_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS approved_by_id BIGINT,
    ADD COLUMN IF NOT EXISTS mail_sent_at TIMESTAMPTZ;

COMMENT ON COLUMN t_payment_notice.approval_status IS '承認ステータス: NONE/PENDING_APPROVAL/APPROVED/REJECTED';
COMMENT ON COLUMN t_payment_notice.approval_requested_at IS '承認リクエスト日時';
COMMENT ON COLUMN t_payment_notice.approval_requested_by_id IS '承認リクエスト者ID（s_user.id）';
COMMENT ON COLUMN t_payment_notice.approved_at IS '承認日時';
COMMENT ON COLUMN t_payment_notice.approved_by_id IS '承認者ID（s_user.id）';
COMMENT ON COLUMN t_payment_notice.mail_sent_at IS 'パートナーへのメール送付日時';

-- デフォルト値を設定
UPDATE s_company_info SET notice_approval_threshold = 500000, token_expiry_days = 14 WHERE notice_approval_threshold IS NULL;
