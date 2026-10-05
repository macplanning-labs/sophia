-- ============================================================
-- 052_client_invoice_receipt_confirmed_template.sql
-- クライアント向け請求書の受領確認通知メール（社内向け）を追加する。
-- ============================================================
INSERT INTO s_email_template (code, subject, body, description) VALUES
    ('client_invoice_receipt_confirmed',
     '【請求書受領確認】{{ invoice_id }}',
     E'{{ client_name }} 様より、以下の請求書について受領確認がありました。\n\n■請求書番号：{{ invoice_id }}\n■対象年月：{{ target_month }}\n■税込合計：¥{{ total_amount }}\n\nご確認ください。',
     'クライアント向け請求書 受領確認通知メール（社内向け）')
ON CONFLICT (code) DO UPDATE SET
    subject = EXCLUDED.subject,
    body = EXCLUDED.body,
    description = EXCLUDED.description;
