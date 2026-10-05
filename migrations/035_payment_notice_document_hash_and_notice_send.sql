-- ============================================================
-- 035_payment_notice_document_hash_and_notice_send.sql
-- ============================================================
-- 1) パートナー承諾時の document_hash 保存用カラム（ポータル承諾で参照済みだが未追加だった）
-- 2) notice_send テンプレートを「支払通知書＋代理請求書の確認URL」文面へ更新

ALTER TABLE t_payment_notice
    ADD COLUMN IF NOT EXISTS document_hash VARCHAR(64) NOT NULL DEFAULT '';

COMMENT ON COLUMN t_payment_notice.document_hash IS 'パートナー承諾時に保存した請求書PDFのSHA256ハッシュ（電子帳簿保存法）';

UPDATE s_email_template
SET subject = '【{{ company_name }}】{{ month }}分 支払通知書・請求書のご確認（{{ notice_id }}）',
    body = E'{{ partner_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の支払通知書、および当社が貴社に代わり作成した請求書をご確認ください。\n下記URLより内容をご確認のうえ、ご承諾をお願いいたします。\n\n▼ 支払通知書・請求書 確認・承諾URL\n{{ token_url }}\n\n■通知書番号：{{ notice_id }}\n\nよろしくお願いいたします。\n\n--------------------------------------------------\n{{ company_name }}\n--------------------------------------------------',
    description = '支払通知書・パートナー代理請求書送付メール（自社→パートナー。確認用トークンURL付き。PDF添付なし）',
    updated_at = NOW()
WHERE code = 'notice_send';
