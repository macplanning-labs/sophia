-- ============================================================
-- 030_payroll_view_permission.sql — 給与閲覧権限をis_staff(管理者)から分離
-- ============================================================
-- これまで「管理者(is_staff=true)」であれば全社員の給与明細を閲覧・確認・
-- 振込済み操作できたが、管理者権限（発注承認・ユーザー管理・マスタメンテ等）と
-- 給与閲覧権限は本来別物であり、ユーザー管理で管理者を付与すると意図せず
-- 給与情報まで見えてしまう問題があったため分離する。
--
-- デフォルトはFALSE（誰も見られない）。既存の管理者アカウントは今回のマイグレーション
-- では自動的にはONにしない — 実際に給与処理を行うアカウントにのみ個別に付与する。

ALTER TABLE s_user
    ADD COLUMN IF NOT EXISTS can_view_all_payroll BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN s_user.can_view_all_payroll IS '全社員の給与データを閲覧・確認・振込済み操作できるか（is_staffとは独立した権限）';

-- 移行時点で給与処理を担当している吉川さんのアカウントにのみ付与する
UPDATE s_user SET can_view_all_payroll = TRUE WHERE email = 'y.yoshikawa@example.com';
