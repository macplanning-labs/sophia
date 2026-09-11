-- 044_project_edi_alias.sql
-- EDI注文書の案件名（短縮名など）を案件マスタへ紐するための別名。
-- 照合: BTRIM(ro.project_name) = BTRIM(m_project.name)
--     OR BTRIM(ro.project_name) = BTRIM(m_project.edi_project_alias)

ALTER TABLE m_project
  ADD COLUMN IF NOT EXISTS edi_project_alias VARCHAR(200) NOT NULL DEFAULT '';

COMMENT ON COLUMN m_project.edi_project_alias IS
  'EDI注文書の案件名別名（例: AJIS）。空文字は未設定。正式案件名と異なる場合に設定する。';

-- 既知の表記ゆれ（AJS案件）
UPDATE m_project
   SET edi_project_alias = 'AJIS'
 WHERE project_id = 'PRJ00000001'
   AND (edi_project_alias IS NULL OR edi_project_alias = '');
