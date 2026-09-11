-- ============================================================
-- 042_work_report_request_internal_notify_template.sql
-- 稼働報告 初回依頼 送り忘れ防止の社内通知メール（work_report_request_internal_notify）
-- ============================================================
-- パートナーへは送らず、社内（EmailService::get_notify_email の宛先）へ、
-- 「そろそろ初回依頼メールを送るタイミングです」と気付かせるための通知。
-- クロス等から受け取った勤務表PDFを添付して手動で送る運用のため、自動送信はしない
-- （041のコメント参照。2026-07-30、ユーザー指摘によりこの設計に変更）。

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('work_report_request_internal_notify',
     '【要対応】{{ month }} 分 稼働報告 初回依頼のタイミングです（{{ partner_name }}）',
     E'{{ partner_name }}（{{ project_name }}）の {{ month }} 分について、\n稼働報告書の初回依頼メールを送るタイミングになりました。\n\nクロス等から受け取った勤務表PDFをお手元にご用意のうえ、\n発注書詳細画面の「稼働報告の提出依頼メール」より送信してください。\n\n■発注書：{{ order_id }}\n■発注書詳細URL：{{ order_url }}',
     '稼働報告 初回依頼の送り忘れ防止 社内通知メール（パートナーへは送らない）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject,
    body = EXCLUDED.body,
    description = EXCLUDED.description;
