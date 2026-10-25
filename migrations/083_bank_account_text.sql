-- 083: 口座番号の 3 列を VARCHAR(20) から TEXT に広げる(DEMO-000233 項目4 / S2-G1)。
--
-- 暗号化した値(`enc1:<鍵ID>:<base64>`)は 20 文字を超えるため、同じ列に入るように長さを広げる。
-- データは変えない。VARCHAR(n) から TEXT への変更は長さの拡張だけで、列の作り直し・行の書き換えは起きない。
-- 型が VARCHAR の時だけ変えるので、何度流しても同じ結果になる(冪等)。
--
-- 対象: s_company_info.account_number(自社の振込先)
--       m_partner.account_number(パートナーの請求先口座)
--       m_employee_bank_account.account_number(社員の振込先)

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 's_company_info' AND column_name = 'account_number' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE s_company_info ALTER COLUMN account_number TYPE TEXT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'm_partner' AND column_name = 'account_number' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE m_partner ALTER COLUMN account_number TYPE TEXT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'm_employee_bank_account' AND column_name = 'account_number' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE m_employee_bank_account ALTER COLUMN account_number TYPE TEXT;
    END IF;
END $$;
