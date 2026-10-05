-- ============================================================
-- 024_fix_order_send_expiry_text.sql — order_send テンプレートの有効期限文言を動的化
-- ============================================================
-- order_send本文に「このURLの有効期限は24時間です」という固定文言があったが、
-- 実際のトークン有効期限は s_company_info.token_expiry_days（デフォルト14日）で
-- 判定しており、実態と乖離していた（2026-07-13、ユーザー指摘）。
-- {{ expiry_days }} 変数で実際の設定値を反映するよう修正する。

UPDATE s_email_template
SET body = E'{{ partner_name }} 様\n\nお世話になっております。\n{{ company_name }}です。\n\n{{ month }} 分の注文書をお送りいたします。\n下記URLより内容をご確認の上、承諾をお願いいたします。\n\n▼ 注文書確認・承諾URL\n{{ token_url }}\n\n※ このURLの有効期限は{{ expiry_days }}日間です。\n\nよろしくお願いいたします。'
WHERE code = 'order_send';
