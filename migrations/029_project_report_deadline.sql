-- ============================================================
-- 029_project_report_deadline.sql — 案件(m_project)への稼働報告提出期限設定の追加
-- ============================================================
-- 従来 m_client_contract.report_deadline_days_before（月末からのN営業日前、相対値のみ）
-- で運用してきたが、クライアントによっては「稼働終了月の月末からN営業日前」ではなく
-- 「当月N日までに見込みで提出」という固定日型の運用（例: 一部クライアント案件）があるため、
-- 案件（m_project）単位で種別を選べるようにする。
--
-- NULL（report_deadline_value）の場合はプロジェクト単位の設定が未入力であることを示し、
-- 呼び出し側は従来通り契約(m_client_contract.report_deadline_days_before)の値にフォールバック
-- する想定（このマイグレーション自体は追加のみで、既存の契約側カラムはそのまま残す）。

ALTER TABLE m_project
    ADD COLUMN IF NOT EXISTS report_deadline_type VARCHAR(10) NOT NULL DEFAULT 'RELATIVE',
    ADD COLUMN IF NOT EXISTS report_deadline_value INTEGER,
    ADD COLUMN IF NOT EXISTS report_deadline_holiday_rule VARCHAR(24);

COMMENT ON COLUMN m_project.report_deadline_type IS '稼働報告提出期限の種類: RELATIVE(月末相対) | FIXED_DAY(当月N日指定)';
COMMENT ON COLUMN m_project.report_deadline_value IS 'RELATIVE: 月末からのN営業日前 / FIXED_DAY: 当月N日。NULLはプロジェクト側未設定（契約側にフォールバック）';
COMMENT ON COLUMN m_project.report_deadline_holiday_rule IS 'FIXED_DAYが土日祝の場合の調整: PREVIOUS_BUSINESS_DAY | NEXT_BUSINESS_DAY（RELATIVEの場合はNULL）';
