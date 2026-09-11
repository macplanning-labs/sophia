-- ============================================================
-- 026_fix_reminder_templates_and_cleanup.sql — 未使用テンプレート整理 + 請求書承諾リマインド追加
-- ============================================================
-- ユーザー確認の結果（2026-07-13）:
--   - invoice_send: client_invoice_send があるので不要 → 削除
--   - order_accepted: order_approve があるので不要 → 削除
--   - report_reminder(稼働報告提出リマインド): 未実装機能。実装するには設計が必要な
--     ため、テンプレートには一切手を入れない（削除も追加もしない）
--   - report_send: クライアントへ稼働報告書を送付するメールとして必要。こちらが正。
--     WORK_REPORT_SHARE（025で追加、received_orders/action.rsが誤って参照していた
--     もの）が不要 → 削除し、コード側をreport_send参照に修正済み
--
-- 加えて、reminder_service.rsが呼ぶ"invoice_approve_reminder"はどのmigrationにも
-- 存在せず、請求書承諾リマインドが常に送信失敗していたため追加する
-- （削除済み旧EDI_MPの同名テンプレート文面を復元。email_service.rs側もtoken_url等を
-- 渡すよう拡張済み）。

-- ── 未使用テンプレートの削除 ──
DELETE FROM s_email_template WHERE code IN ('invoice_send', 'order_accepted', 'WORK_REPORT_SHARE');

-- ── 支払通知書承諾リマインド（パートナー宛）──
INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('invoice_approve_reminder',
     '【{{ company_name }}】請求書ご承諾のお願い（請求番号：{{ invoice_id }}）',
     E'{{ partner_name }}　御中\n\nいつもお世話になっております。\n{{ company_name }}でございます。\n\n支払通知書・請求書をお送りしておりますが、まだご承諾の確認がとれておりません。\n\n■ 請求番号：{{ invoice_id }}\n■ 対象月：{{ target_month }}\n■ 税込合計：¥{{ total_amount }}\n\n内容にご同意いただける場合は、下記URLより「承諾」ボタンを押してください。\n\n▼ 支払通知書確認・承諾URL\n{{ token_url }}\n\n--------------------------------------------------\n{{ company_name }}\nTEL: {{ company_tel }}\n--------------------------------------------------',
     '支払通知書承諾リマインドメール（パートナー宛。EDI_MP invoice_approve_reminder を復元）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject, body = EXCLUDED.body, description = EXCLUDED.description;
