-- ============================================================
-- 002_seed_workflow.sql — ワークフロー初期データ
-- ============================================================
-- PARTNER_CYCLE: パートナー発注サイクル（7ステップ）
-- CLIENT_CYCLE: クライアント受注サイクル（5ステップ）
-- ============================================================

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- パートナー発注サイクル
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

INSERT INTO m_workflow_definition (code, name, description) VALUES
    ('PARTNER_CYCLE', 'パートナー発注サイクル', '注文書起票から支払完了までのワークフロー')
ON CONFLICT (code) DO NOTHING;

-- ステップ定義
INSERT INTO m_workflow_step (definition_id, code, name, sort_order, is_terminal, icon, allowed_mail_types)
SELECT d.id, s.code, s.name, s.sort_order, s.is_terminal, s.icon, s.allowed_mail_types::jsonb
FROM m_workflow_definition d
CROSS JOIN (VALUES
    ('DRAFT',            '起票',       1, false, '📝', '[]'),
    ('SENT',             '送付済',     2, false, '📨', '["order_send"]'),
    ('ACCEPTED',         '受諾済',     3, false, '✅', '[]'),
    ('REPORT_RECEIVED',  '報告書受領', 4, false, '📋', '[]'),
    ('NOTICE_CREATED',   '通知作成',   5, false, '💰', '["notice_send"]'),
    ('NOTICE_CONFIRMED', '通知受諾',   6, false, '🤝', '[]'),
    ('PAID',             '支払済',     7, true,  '💵', '[]')
) AS s(code, name, sort_order, is_terminal, icon, allowed_mail_types)
WHERE d.code = 'PARTNER_CYCLE'
ON CONFLICT (definition_id, code) DO NOTHING;

-- 遷移ルール（前進）
INSERT INTO m_workflow_transition (from_step_id, to_step_id, is_back, description)
SELECT s1.id, s2.id, false, s1.name || ' → ' || s2.name
FROM m_workflow_step s1
JOIN m_workflow_step s2 ON s1.definition_id = s2.definition_id AND s2.sort_order = s1.sort_order + 1
JOIN m_workflow_definition d ON s1.definition_id = d.id
WHERE d.code = 'PARTNER_CYCLE'
ON CONFLICT (from_step_id, to_step_id) DO NOTHING;

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- クライアント受注サイクル
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

INSERT INTO m_workflow_definition (code, name, description) VALUES
    ('CLIENT_CYCLE', 'クライアント受注サイクル', '受注登録から入金確認までのワークフロー')
ON CONFLICT (code) DO NOTHING;

INSERT INTO m_workflow_step (definition_id, code, name, sort_order, is_terminal, icon, allowed_mail_types)
SELECT d.id, s.code, s.name, s.sort_order, s.is_terminal, s.icon, s.allowed_mail_types::jsonb
FROM m_workflow_definition d
CROSS JOIN (VALUES
    ('REGISTERED',      '受注登録',   1, false, '📥', '[]'),
    ('REPORT_RECEIVED', '勤怠受領',   2, false, '📋', '["report_send"]'),
    ('REPORT_SENT',     '報告書送付', 3, false, '📨', '[]'),
    ('INVOICED',        '請求書処理', 4, false, '🧾', '["invoice_send"]'),
    ('PAID',            '入金済',     5, true,  '💵', '[]')
) AS s(code, name, sort_order, is_terminal, icon, allowed_mail_types)
WHERE d.code = 'CLIENT_CYCLE'
ON CONFLICT (definition_id, code) DO NOTHING;

INSERT INTO m_workflow_transition (from_step_id, to_step_id, is_back, description)
SELECT s1.id, s2.id, false, s1.name || ' → ' || s2.name
FROM m_workflow_step s1
JOIN m_workflow_step s2 ON s1.definition_id = s2.definition_id AND s2.sort_order = s1.sort_order + 1
JOIN m_workflow_definition d ON s1.definition_id = d.id
WHERE d.code = 'CLIENT_CYCLE'
ON CONFLICT (from_step_id, to_step_id) DO NOTHING;
