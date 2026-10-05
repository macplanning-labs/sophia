-- 046: 案件に紐づかない社員向け「月次稼働報告（自己申告）」対応
--
-- 稼働報告（t_monthly_timesheet）は client_contract_id が NOT NULL で受注契約
-- （＝案件）に強制紐付けされており、案件を持たない社員（バックオフィス等）は
-- 稼働報告を登録する手段がなかった。
-- 新テーブルは作らず、既存テーブルに employee_id（社員直接紐付け）を追加し、
-- client_contract_id と employee_id のどちらか一方だけを必須にする。

ALTER TABLE t_monthly_timesheet
    ALTER COLUMN client_contract_id DROP NOT NULL;

ALTER TABLE t_monthly_timesheet
    ADD COLUMN IF NOT EXISTS employee_id BIGINT REFERENCES m_employee(id) ON DELETE CASCADE;

-- Googleスプレッドシート連携（自己申告フォームの「Googleスプレッドシートで入力」経路）で
-- 複製したテンプレートのファイルIDを保持する。提出（取り込み）前の下書き状態でも
-- どのシートに紐づくかを追跡できるようにする。
ALTER TABLE t_monthly_timesheet
    ADD COLUMN IF NOT EXISTS sheet_file_id VARCHAR(100) NOT NULL DEFAULT '';

-- client_contract_id 経由（案件紐付け）と employee_id 経由（社員自己申告）の
-- どちらか一方だけが設定されている状態を保証する
ALTER TABLE t_monthly_timesheet
    DROP CONSTRAINT IF EXISTS chk_timesheet_owner;
ALTER TABLE t_monthly_timesheet
    ADD CONSTRAINT chk_timesheet_owner CHECK (
        (client_contract_id IS NOT NULL AND employee_id IS NULL) OR
        (client_contract_id IS NULL AND employee_id IS NOT NULL)
    );

-- 社員×対象月の一意性（案件紐付け側は既存の UNIQUE(client_contract_id, target_month) のまま）
CREATE UNIQUE INDEX IF NOT EXISTS idx_monthly_timesheet_employee_month
    ON t_monthly_timesheet (employee_id, target_month)
    WHERE employee_id IS NOT NULL;

COMMENT ON COLUMN t_monthly_timesheet.employee_id IS '案件非依存の社員自己申告の場合の紐付け先。client_contract_idとは排他（chk_timesheet_owner）。';
COMMENT ON COLUMN t_monthly_timesheet.sheet_file_id IS 'Googleスプレッドシート連携で複製したテンプレートのファイルID。未使用時は空文字。';
