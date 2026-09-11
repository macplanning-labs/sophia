-- ============================================================
-- 040_work_report_request_template.sql
-- 稼働報告 初回依頼メール（work_report_request）新規追加
-- ============================================================
-- work_report_reminder は「まだご登録いただけておりません」という、既に未提出で
-- あることを前提にした文面のため、初めて提出をお願いする場合に使うと失礼にあたる
-- （2026-07-30、パートナーへ初回依頼のつもりでリマインド文面を送ってしまい、
--   ユーザーが手動で文面を修正して送り直した実例あり）。
-- 発注書詳細の「稼働報告の提出依頼メール」ボタン専用に、中立的な文面のテンプレートを
-- 新設し、work_report_reminder とは完全に分離する。

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('work_report_request',
     '【稼働報告書登録のお願い】{{ month }} 分の稼働報告書（勤務表）をご登録ください',
     E'{{ partner_name }} 御中\n\nいつもお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の稼働報告書（勤務表）のご登録をお願いいたします。\n下記URLより登録をお願いいたします。\n\n▼ 稼働報告書（勤務表） 登録URL\n{{ token_url }}\n\n《画面の見方》\n上記URLを開くと、「稼働報告書の提出」という欄が表示されます。\n枠内にファイル（Excel／PDF）をドラッグ＆ドロップ、またはクリックして選択し、「稼働報告書を提出する」ボタンを押してください。\n\n《お願い》\n{{ deadline_date }}までのご登録にご協力をお願いいたします。\nご不明な点がございましたら、本メールにご返信ください。\n\n以上、よろしくお願いいたします。',
     '稼働報告書 初回登録依頼メール（自社→パートナー。work_report_reminderとは別に新設。未提出前提の文言を含まない）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject,
    body = EXCLUDED.body,
    description = EXCLUDED.description;
