-- ============================================================
-- 032_expense_view_permission.sql — 経費閲覧権限をis_staff(管理者)から分離
-- ============================================================
-- 経費申請の一覧・詳細APIには従来フィルタが一切なく、ログインしていれば
-- 一般社員でも管理者でも全員分の経費が見えてしまっていた（承認・差戻しも同様）。
-- 給与閲覧権限(can_view_all_payroll, migrations/030)と同じ考え方で、
-- 経費の全件閲覧・承認・差戻し権限をis_staffとは独立したフラグに分離する。
--
-- デフォルトはFALSE（誰も見られない＝一般社員は自分の申請のみ）。
-- 移行時点で経費処理を担当している吉川さんのアカウントにのみ付与する。

ALTER TABLE s_user
    ADD COLUMN IF NOT EXISTS can_view_all_expenses BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN s_user.can_view_all_expenses IS '全社員の経費申請を閲覧・承認・差戻しできるか（is_staffとは独立した権限）';

UPDATE s_user SET can_view_all_expenses = TRUE WHERE email = 'y.yoshikawa@example.com';
