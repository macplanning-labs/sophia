-- ============================================================
-- 022_client_invoice_send_template.sql — クライアント向け請求書送付メールテンプレート
-- ============================================================
-- 既存の "invoice_send" テンプレートは実際にはパートナー向け「支払通知・請求書送付」
-- （notices.rs::send_mail が使用、自社→パートナー）であり、クライアント向けの
-- 「請求書送付」メールテンプレートはこれまで存在しなかった（021の請求書承認・送信
-- ワークフロー実装時に判明）。新規コードとして client_invoice_send を追加する。

INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('client_invoice_send',
     '【{{ company_name }}】{{ month }}分 請求書送付のご案内（請求番号：{{ invoice_id }}）',
     E'{{ client_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の請求書をお送りいたします。\nご確認の上、お支払い手続きをお願いいたします。\n\n請求書番号: {{ invoice_id }}\n請求金額: ¥{{ total }}\nお支払期限: {{ due_date }}\n\nよろしくお願いいたします。',
     'クライアント向け請求書送付メール（自社→クライアント、請求書承認・送信ワークフロー用）')
ON CONFLICT (code) DO NOTHING;
