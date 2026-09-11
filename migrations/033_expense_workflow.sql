-- ============================================================
-- 033_expense_workflow.sql — 経費申請ワークフロー定義
-- ============================================================
-- 汎用ワークフロー基盤（m_workflow_* / h_workflow_log）に
-- 経費申請（EXPENSE_REQUEST）の定義・遷移を登録する。
-- アプリ側の document_workflow がステータス遷移時にステップIDを解決してログする。
-- ============================================================

INSERT INTO m_workflow_definition (code, name, description) VALUES
    ('EXPENSE_REQUEST', '経費申請', '経費申請の申請〜承認〜精算までのワークフロー')
ON CONFLICT (code) DO NOTHING;

INSERT INTO m_workflow_step (definition_id, code, name, sort_order, is_terminal, icon, allowed_mail_types)
SELECT d.id, s.code, s.name, s.sort_order, s.is_terminal, s.icon, s.allowed_mail_types::jsonb
FROM m_workflow_definition d
CROSS JOIN (VALUES
    ('DRAFT',    '下書き', 1, false, '📝', '[]'),
    ('PENDING',  '申請中', 2, false, '📨', '[]'),
    ('APPROVED', '承認済', 3, false, '✅', '[]'),
    ('REJECTED', '却下',   4, false, '↩️', '[]'),
    ('PAID',     '精算済', 5, true,  '💵', '[]')
) AS s(code, name, sort_order, is_terminal, icon, allowed_mail_types)
WHERE d.code = 'EXPENSE_REQUEST'
ON CONFLICT (definition_id, code) DO NOTHING;

-- 前進: DRAFT → PENDING → APPROVED → PAID
INSERT INTO m_workflow_transition (from_step_id, to_step_id, is_back, description)
SELECT s1.id, s2.id, false, s1.name || ' → ' || s2.name
FROM m_workflow_step s1
JOIN m_workflow_step s2 ON s1.definition_id = s2.definition_id
JOIN m_workflow_definition d ON s1.definition_id = d.id
WHERE d.code = 'EXPENSE_REQUEST'
  AND (
    (s1.code = 'DRAFT' AND s2.code = 'PENDING')
    OR (s1.code = 'PENDING' AND s2.code = 'APPROVED')
    OR (s1.code = 'APPROVED' AND s2.code = 'PAID')
  )
ON CONFLICT (from_step_id, to_step_id) DO NOTHING;

-- 却下: PENDING → REJECTED
INSERT INTO m_workflow_transition (from_step_id, to_step_id, is_back, description)
SELECT s1.id, s2.id, false, '申請中 → 却下'
FROM m_workflow_step s1
JOIN m_workflow_step s2 ON s1.definition_id = s2.definition_id
JOIN m_workflow_definition d ON s1.definition_id = d.id
WHERE d.code = 'EXPENSE_REQUEST' AND s1.code = 'PENDING' AND s2.code = 'REJECTED'
ON CONFLICT (from_step_id, to_step_id) DO NOTHING;

-- 再申請: REJECTED → PENDING
INSERT INTO m_workflow_transition (from_step_id, to_step_id, is_back, description)
SELECT s1.id, s2.id, true, '却下 → 申請中（再申請）'
FROM m_workflow_step s1
JOIN m_workflow_step s2 ON s1.definition_id = s2.definition_id
JOIN m_workflow_definition d ON s1.definition_id = d.id
WHERE d.code = 'EXPENSE_REQUEST' AND s1.code = 'REJECTED' AND s2.code = 'PENDING'
ON CONFLICT (from_step_id, to_step_id) DO NOTHING;

-- 承認取り消し: APPROVED → PENDING（未精算時のみ。アプリ側で PAID をガード）
INSERT INTO m_workflow_transition (from_step_id, to_step_id, is_back, description)
SELECT s1.id, s2.id, true, '承認済 → 申請中（承認取り消し）'
FROM m_workflow_step s1
JOIN m_workflow_step s2 ON s1.definition_id = s2.definition_id
JOIN m_workflow_definition d ON s1.definition_id = d.id
WHERE d.code = 'EXPENSE_REQUEST' AND s1.code = 'APPROVED' AND s2.code = 'PENDING'
ON CONFLICT (from_step_id, to_step_id) DO NOTHING;
