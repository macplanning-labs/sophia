-- ============================================================
-- 043_client_invoice_token_send.sql
-- クライアント向け請求書送付メールを、支払通知書（notice_send）と同じ
-- トークンURL方式に変更する。
-- ============================================================
-- 背景（2026-07-30、ユーザー指摘）:
-- 請求書PDFをメールに直接添付する現行方式はセキュリティ上望ましくないため、
-- パートナー向け支払通知書と同様に、トークンURL経由でSophia上の確認画面を開き、
-- 画面上のボタンからPDFをダウンロードする方式に統一する（PDF添付は廃止）。
--
-- t_billing_invoice に t_payment_notice と同じ形で uuid（トークン）列を追加する。
ALTER TABLE t_billing_invoice
    ADD COLUMN IF NOT EXISTS uuid UUID NOT NULL UNIQUE DEFAULT gen_random_uuid();

COMMENT ON COLUMN t_billing_invoice.uuid IS 'トークンURL（/token/{uuid}）でのクライアント確認ページアクセスに使用';

-- client_invoice_send: 2回目以降（同一クライアントへの送信実績あり）向けの簡潔な文面。
-- PDF添付ではなく、トークンURL経由でのダウンロード案内に変更。
UPDATE s_email_template
SET subject = '【{{ company_name }}】{{ month }}分 請求書発行のお知らせ（請求番号：{{ invoice_id }}）',
    body = E'{{ client_name }} 様\n\nいつも大変お世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の請求書を発行いたしました。\n下記URLよりPDFファイルをダウンロードの上、ご確認いただけますようお願い申し上げます。\n\n■請求書ダウンロードURL\n{{ token_url }}\n※有効期限：{{ deadline_date }}まで\n\n■お支払いについて\n・請求書番号：{{ invoice_id }}\n・御請求金額：{{ total }}円（税込）\n・お支払期日：{{ due_date }}\n\nよろしくお願いいたします。',
    description = 'クライアント向け請求書送付メール（2回目以降。トークンURL経由でのPDFダウンロード案内。PDF添付なし）',
    updated_at = NOW()
WHERE code = 'client_invoice_send';

-- client_invoice_send_first: 初回送信（このクライアントへの送信実績が過去に一件もない）向け。
-- EDIシステム「Sophia」導入の説明・セキュリティ強化の趣旨を含む文面。
INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('client_invoice_send_first',
     '【請求書ご案内】{{ month }}分 御請求書発行のお知らせ（{{ company_name }}より自動送信）',
     E'{{ client_name }} 様\n\nいつも大変お世話になっております。\n{{ company_name }}でございます。\n\n{{ month }} 分につきまして、御請求書を発行いたしました。\n弊社では、セキュリティ強化および情報保護の観点から、請求書（PDF）を弊社開発のEDIシステム「Sophia」にて公開しております。\n大変お手数ではございますが、以下のURLよりPDFファイルをダウンロードの上、ご確認いただけますようお願い申し上げます。\n\n■請求書ダウンロードURL\n{{ token_url }}\n※有効期限：{{ deadline_date }}まで\n\n■お支払いについて\n・請求書番号：{{ invoice_id }}\n・御請求金額：{{ total }}円（税込）\n・お支払期日：{{ due_date }}\n\n※本メールは、弊社請求管理システム「Sophia」より自動送信しております。\nURLが開けないなど不具合がございましたら、お手数ですが弊社担当まで直接ご連絡ください。\n\nよろしくお願い申し上げます。',
     'クライアント向け請求書送付メール（初回。EDIシステム「Sophia」導入案内の文言付き。PDF添付なし）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject,
    body = EXCLUDED.body,
    description = EXCLUDED.description;
