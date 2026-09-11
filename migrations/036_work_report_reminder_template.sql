-- ============================================================
-- 036_work_report_reminder_template.sql
-- 稼働報告提出依頼メール（パートナー宛）テンプレート追加
-- ============================================================
-- HANDOVER方針により既存の report_reminder は触らない。
-- reminder_service / 手動「提出依頼メール」が参照する work_report_reminder を新規追加する。
-- token_url にはパートナーポータルの稼働報告画面 URL（{BASE_URL}/portal/timesheets）を渡す。

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('work_report_reminder',
     '【稼働報告提出依頼】{{ month }} 分の稼働報告書をご提出ください',
     E'{{ partner_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の稼働報告書がまだ届いておりません。\nお忙しいところ恐縮ですが、下記URLよりパートナーポータルへアクセスのうえ、ご提出をお願いいたします。\n\n▼ 稼働報告書 提出URL\n{{ token_url }}\n\nよろしくお願いいたします。\n\n--------------------------------------------------\n{{ company_name }}\nTEL: {{ company_tel }}\n--------------------------------------------------',
     '稼働報告提出依頼メール（パートナー宛。ポータル /portal/timesheets へのリンク付き）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject,
    body = EXCLUDED.body,
    description = EXCLUDED.description;
