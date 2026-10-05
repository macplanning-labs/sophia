-- 060_company_margin_targets.sql
-- 自社情報(s_company_info)に利益の目安を追加

ALTER TABLE s_company_info ADD COLUMN IF NOT EXISTS target_margin_direct INT NOT NULL DEFAULT 20
  CHECK (target_margin_direct >= 0 AND target_margin_direct <= 100);

ALTER TABLE s_company_info ADD COLUMN IF NOT EXISTS target_margin_subcontract INT NOT NULL DEFAULT 8
  CHECK (target_margin_subcontract >= 0 AND target_margin_subcontract <= 100);
