-- 059_project_commercial_flow.sql
-- 案件(m_project)に「商流」(直営/下請)と「テスト案件」の印を追加する。
-- 既存データは書き換えない。

ALTER TABLE m_project
    ADD COLUMN IF NOT EXISTS commercial_flow VARCHAR(12) NULL CHECK (commercial_flow IN ('DIRECT','SUBCONTRACT')),
    ADD COLUMN IF NOT EXISTS is_test BOOLEAN NOT NULL DEFAULT FALSE;
