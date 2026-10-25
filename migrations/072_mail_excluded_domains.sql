-- 072_mail_excluded_domains.sql
-- 自社のメールドメインなど、「取引先のメール」として扱わないドメインを設定できるようにする(DEMO-000164)。
-- 1行に1ドメイン(カンマ・空白区切りも可)。空なら、これまでどおり(除外なし)。
ALTER TABLE s_company_info
    ADD COLUMN IF NOT EXISTS excluded_mail_domains TEXT NOT NULL DEFAULT '';
