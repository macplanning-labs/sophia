-- 005_user_profile_employee_id.sql
-- s_user_profile に employee_id カラムを追加（ロールベースアクセス制御）
--
-- Employee ロール判定: employee_id IS NOT NULL → Role::Employee
-- 1ユーザーにつき1社員（UNIQUE）

ALTER TABLE s_user_profile
    ADD COLUMN IF NOT EXISTS employee_id BIGINT REFERENCES m_employee(id) ON DELETE SET NULL;

-- 1社員につき1アカウント（重複防止）
CREATE UNIQUE INDEX IF NOT EXISTS idx_user_profile_employee_id
    ON s_user_profile(employee_id) WHERE employee_id IS NOT NULL;
