-- 071_api_key_expiry.sql — 外部向けWebAPIのAPIキーに、有効期限(任意)を足す(DEMO-000167 項目7)
--
-- 追加のみ。既存のキーはすべて NULL(無期限)のままで、動作は変わらない。
-- 過去の時刻になったキーは認証できない(認証ミドルウェアで判定)。
ALTER TABLE t_company_api_key ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ NULL;
COMMENT ON COLUMN t_company_api_key.expires_at IS 'NULL=無期限(既存のキーはすべてNULL)。この時刻以降は認証できない';
