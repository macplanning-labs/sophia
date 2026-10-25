-- 073_owner_stamps_not_null.sql — 担当スタンプを必須にする(DEMO-000173 PR 7b)
-- 設計: docs/design/詳細設計書_ロールと組織に基づく閲覧範囲.md §3.3(設計時の番号は 070。071・072 が先に入ったため 073 にした)
--
-- 前提: 担当が空の行が 0 件であること(本番 2026-10-11 確認済み)。1 件でも残っていると、このマイグレーションが
--       失敗し、アプリが起動しない。デプロイスクリプトが反映前に件数を検査して止める(deploy_check_owner_nulls)。
-- 稼働報告(t_monthly_timesheet)だけは、社員の自己申告の行(employee_id があり、担当が無い)を認めるため、
-- NOT NULL ではなく CHECK にする。

ALTER TABLE m_client
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE m_project
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE m_client_contract
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE m_partner_contract
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE t_purchase_order
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE t_payment_notice
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE t_received_order
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE t_billing_invoice
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;
ALTER TABLE t_monthly_task
    ALTER COLUMN owner_org_unit_id SET NOT NULL,
    ALTER COLUMN owner_employee_id SET NOT NULL;

ALTER TABLE t_monthly_timesheet
    ADD CONSTRAINT chk_monthly_timesheet_owner_org CHECK (owner_org_unit_id IS NOT NULL OR employee_id IS NOT NULL),
    ADD CONSTRAINT chk_monthly_timesheet_owner_emp CHECK (owner_employee_id IS NOT NULL OR employee_id IS NOT NULL);
