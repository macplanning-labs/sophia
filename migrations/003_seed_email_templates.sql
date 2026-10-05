-- ============================================================
-- 003_seed_email_templates.sql — メールテンプレート初期データ
-- ============================================================

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('order_send', '【注文書送付】{{ partner_name }} 様', E'{{ partner_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の注文書をお送りいたします。\n下記URLより内容をご確認の上、承諾をお願いいたします。\n\n▼ 注文書確認・承諾URL\n{{ token_url }}\n\n※ このURLの有効期限は24時間です。\n\nよろしくお願いいたします。', '注文書送付メール'),

    ('order_accepted', '【注文書承諾】{{ partner_name }} 様より承諾されました', E'{{ partner_name }} 様より注文書が承諾されました。\n\n注文書番号: {{ order_id }}\n承諾日時: {{ accepted_at }}\n\nご確認ください。', '注文書承諾通知（社内向け）'),

    ('notice_send', '【支払通知書】{{ partner_name }} 様', E'{{ partner_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の支払通知書をお送りいたします。\n下記URLより内容をご確認ください。\n\n▼ 支払通知書確認URL\n{{ token_url }}\n\nよろしくお願いいたします。', '支払通知書送付メール'),

    ('report_reminder', '【稼働報告リマインド】{{ month }} 分の稼働報告書をお送りください', E'{{ partner_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の稼働報告書がまだ届いておりません。\nお忙しいところ恐縮ですが、ご提出をお願いいたします。\n\nよろしくお願いいたします。', '稼働報告リマインドメール'),

    ('invoice_send', '【請求書送付】{{ client_name }} 様', E'{{ client_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の請求書をお送りいたします。\nご確認の上、お支払い手続きをお願いいたします。\n\n請求書番号: {{ invoice_id }}\n請求金額: ¥{{ total }}\nお支払期限: {{ due_date }}\n\nよろしくお願いいたします。', '請求書送付メール'),

    ('report_send', '【稼働報告書送付】{{ client_name }} 様', E'{{ client_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の稼働報告書をお送りいたします。\nご確認をお願いいたします。\n\nよろしくお願いいたします。', '稼働報告書クライアント送付メール')
ON CONFLICT (code) DO NOTHING;
