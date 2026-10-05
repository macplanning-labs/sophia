-- 065_client_invoice_issued_by_client.sql
-- 「この取引先の請求書は、当社ではなく先方のシステムで作る（作成代行）」を、取引先マスタの汎用の項目にする（DEMO-000148）。
-- これまでは EDI 方式の値（特定の取引先の EDI システム名）と一致するかで判定していた。
-- 判定に取引先のシステム名を使わず、この項目だけを見るようにする。
ALTER TABLE m_client
    ADD COLUMN IF NOT EXISTS invoice_issued_by_client BOOLEAN NOT NULL DEFAULT FALSE;

-- 既存のデータの引き継ぎ: EDI 方式が「なし」「メール」以外（＝先方が独自の EDI システムを持ち、請求書もそこで作る）の取引先に印を付ける。
-- 「メール」は、注文書がメールで届くというだけで、請求書は当社が作る。
UPDATE m_client
SET invoice_issued_by_client = TRUE
WHERE edi_system_type NOT IN ('', 'EMAIL');
