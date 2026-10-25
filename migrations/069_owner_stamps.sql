-- 069_owner_stamps.sql — データの担当スタンプ(DEMO-000173 PR 6)
-- 設計: docs/design/設計書_ロールと組織に基づく閲覧範囲.md §6.5・§6.6、詳細設計書 §3.2
--
-- 権限管理の対象データに「担当組織」「担当者」の2列を足す。
--  * すべて NULL 可(既存データは NULL のまま。埋めるのは PR 7 の作業、NOT NULL 化は 070)
--  * 明細表・m_partner・m_engineer・受信メールには足さない(設計書 §6.5・§2.3)
-- 自動取込の「受け皿」(D16)は s_company_info に NULL 可で持つ。

-- m_client(顧客)
ALTER TABLE m_client
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_client_owner_org ON m_client(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_client_owner_emp ON m_client(owner_employee_id);
COMMENT ON COLUMN m_client.owner_org_unit_id IS '担当組織(m_org_unit.id)。顧客の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN m_client.owner_employee_id IS '担当者(m_employee.id)。顧客の担当スタンプ。DEMO-000173';

-- m_project(案件)
ALTER TABLE m_project
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_project_owner_org ON m_project(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_project_owner_emp ON m_project(owner_employee_id);
COMMENT ON COLUMN m_project.owner_org_unit_id IS '担当組織(m_org_unit.id)。案件の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN m_project.owner_employee_id IS '担当者(m_employee.id)。案件の担当スタンプ。DEMO-000173';

-- m_client_contract(受注契約)
ALTER TABLE m_client_contract
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_client_contract_owner_org ON m_client_contract(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_client_contract_owner_emp ON m_client_contract(owner_employee_id);
COMMENT ON COLUMN m_client_contract.owner_org_unit_id IS '担当組織(m_org_unit.id)。受注契約の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN m_client_contract.owner_employee_id IS '担当者(m_employee.id)。受注契約の担当スタンプ。DEMO-000173';

-- m_partner_contract(発注契約)
ALTER TABLE m_partner_contract
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_partner_contract_owner_org ON m_partner_contract(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_partner_contract_owner_emp ON m_partner_contract(owner_employee_id);
COMMENT ON COLUMN m_partner_contract.owner_org_unit_id IS '担当組織(m_org_unit.id)。発注契約の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN m_partner_contract.owner_employee_id IS '担当者(m_employee.id)。発注契約の担当スタンプ。DEMO-000173';

-- t_purchase_order(発注)
ALTER TABLE t_purchase_order
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_purchase_order_owner_org ON t_purchase_order(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_owner_emp ON t_purchase_order(owner_employee_id);
COMMENT ON COLUMN t_purchase_order.owner_org_unit_id IS '担当組織(m_org_unit.id)。発注の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN t_purchase_order.owner_employee_id IS '担当者(m_employee.id)。発注の担当スタンプ。DEMO-000173';

-- t_payment_notice(支払通知)
ALTER TABLE t_payment_notice
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_payment_notice_owner_org ON t_payment_notice(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_payment_notice_owner_emp ON t_payment_notice(owner_employee_id);
COMMENT ON COLUMN t_payment_notice.owner_org_unit_id IS '担当組織(m_org_unit.id)。支払通知の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN t_payment_notice.owner_employee_id IS '担当者(m_employee.id)。支払通知の担当スタンプ。DEMO-000173';

-- t_received_order(受注)
ALTER TABLE t_received_order
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_received_order_owner_org ON t_received_order(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_received_order_owner_emp ON t_received_order(owner_employee_id);
COMMENT ON COLUMN t_received_order.owner_org_unit_id IS '担当組織(m_org_unit.id)。受注の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN t_received_order.owner_employee_id IS '担当者(m_employee.id)。受注の担当スタンプ。DEMO-000173';

-- t_billing_invoice(請求書)
ALTER TABLE t_billing_invoice
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_billing_invoice_owner_org ON t_billing_invoice(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_billing_invoice_owner_emp ON t_billing_invoice(owner_employee_id);
COMMENT ON COLUMN t_billing_invoice.owner_org_unit_id IS '担当組織(m_org_unit.id)。請求書の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN t_billing_invoice.owner_employee_id IS '担当者(m_employee.id)。請求書の担当スタンプ。DEMO-000173';

-- t_monthly_timesheet(稼働報告)
ALTER TABLE t_monthly_timesheet
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_monthly_timesheet_owner_org ON t_monthly_timesheet(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_monthly_timesheet_owner_emp ON t_monthly_timesheet(owner_employee_id);
COMMENT ON COLUMN t_monthly_timesheet.owner_org_unit_id IS '担当組織(m_org_unit.id)。稼働報告の担当スタンプ。DEMO-000173';
COMMENT ON COLUMN t_monthly_timesheet.owner_employee_id IS '担当者(m_employee.id)。稼働報告の担当スタンプ。DEMO-000173';

-- t_monthly_task(月次タスク)
ALTER TABLE t_monthly_task
    ADD COLUMN IF NOT EXISTS owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
CREATE INDEX IF NOT EXISTS idx_monthly_task_owner_org ON t_monthly_task(owner_org_unit_id);
CREATE INDEX IF NOT EXISTS idx_monthly_task_owner_emp ON t_monthly_task(owner_employee_id);
COMMENT ON COLUMN t_monthly_task.owner_org_unit_id IS '担当組織(m_org_unit.id)。月次タスクの担当スタンプ。DEMO-000173';
COMMENT ON COLUMN t_monthly_task.owner_employee_id IS '担当者(m_employee.id)。月次タスクの担当スタンプ。DEMO-000173';

-- s_company_info: 自動取込の受け皿(D16)。EDI 同期・メール取込で、親から担当を決められないときに押す。
ALTER TABLE s_company_info
    ADD COLUMN IF NOT EXISTS auto_import_owner_org_unit_id BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS auto_import_owner_employee_id BIGINT REFERENCES m_employee(id) ON DELETE RESTRICT;
COMMENT ON COLUMN s_company_info.auto_import_owner_org_unit_id IS '自動取込の受け皿: 担当組織(m_org_unit.id)。D16。DEMO-000173';
COMMENT ON COLUMN s_company_info.auto_import_owner_employee_id IS '自動取込の受け皿: 担当者(m_employee.id)。D16。DEMO-000173';
