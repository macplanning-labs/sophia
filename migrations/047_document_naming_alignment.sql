-- ============================================================
-- 047_document_naming_alignment.sql — 発注書/支払通知書/請求書の命名統一
-- ============================================================
-- 背景: t_purchase_order（自社→パートナー：発注書）/ t_payment_notice
-- （自社→パートナー：支払通知書・代理請求書）/ t_billing_invoice
-- （自社→クライアント：請求書）の3テーブルで、同じ概念に異なるカラム名が
-- 使われていた（属性一覧レビューで確認、2026-08-11合意）。
-- 命名統一と、レビューで見つかった漏れ（drive_file_id, tax_rate）の補完を行う。

-- 1) 承諾日時カラムの統一（3テーブルで別々の名前だったものを揃える）
--    t_purchase_order.finalized_at → partner_accepted_at
ALTER TABLE t_purchase_order RENAME COLUMN finalized_at TO partner_accepted_at;

--    t_payment_notice.confirmed_at → partner_accepted_at
ALTER TABLE t_payment_notice RENAME COLUMN confirmed_at TO partner_accepted_at;

--    t_billing_invoice.confirmed_at（021_invoice_approval_workflowでのstatus
--    拡張以降、書き込み箇所が存在しない死にカラムだった）→ client_accepted_at
--    として再利用開始。クライアント向け請求書承諾フローを新規実装する際に使う。
ALTER TABLE t_billing_invoice RENAME COLUMN confirmed_at TO client_accepted_at;
COMMENT ON COLUMN t_billing_invoice.client_accepted_at IS 'クライアントが請求書内容を承諾した日時（新規実装、旧confirmed_atカラムを再利用）';

-- 2) t_billing_invoiceにdocument_hashを追加
--    t_purchase_order.document_hash / t_payment_notice.document_hash と同じ名前で統一。
--    請求書だけ承諾時のハッシュ保存機能自体が存在しなかった。
ALTER TABLE t_billing_invoice ADD COLUMN IF NOT EXISTS document_hash VARCHAR(64) NOT NULL DEFAULT '';
COMMENT ON COLUMN t_billing_invoice.document_hash IS 'クライアント承諾時に保存する請求書PDFのSHA256ハッシュ（電子帳簿保存法対応の改ざん検知用）';

-- 3) t_payment_notice.confirmed_by_id を削除
--    参照先の s_user への更新処理がコード上どこにも存在しない死にカラム。
ALTER TABLE t_payment_notice DROP COLUMN IF EXISTS confirmed_by_id;

-- 4) t_payment_notice.drive_file_id を追加
--    t_purchase_order / t_billing_invoice にはあるが支払通知書だけ漏れていた（確認済み）。
ALTER TABLE t_payment_notice ADD COLUMN IF NOT EXISTS drive_file_id VARCHAR(200) NOT NULL DEFAULT '';
COMMENT ON COLUMN t_payment_notice.drive_file_id IS 'Google Drive保存先ファイルID（発注書・請求書と同じ運用に揃える）';

-- 5) t_purchase_order_item.tax_rate を追加
--    045_peppol_jp_pint.sql で t_billing_invoice_item / t_payment_notice_item には
--    追加されたが、t_purchase_order_item だけ対象から漏れていた（確認済み）。
ALTER TABLE t_purchase_order_item ADD COLUMN IF NOT EXISTS tax_rate NUMERIC(5,2) NOT NULL DEFAULT 10.00;

-- 6) PDFパス列名の統一: t_billing_invoice.pdf_file → invoice_pdf
--    t_purchase_order.order_pdf / t_payment_notice.notice_pdf と同じ「{文書}_pdf」パターンに揃える。
ALTER TABLE t_billing_invoice RENAME COLUMN pdf_file TO invoice_pdf;

-- 7) 明細金額列名の統一: t_purchase_order_item.price → amount
--    t_payment_notice_item.amount / t_billing_invoice_item.amount と揃える。
ALTER TABLE t_purchase_order_item RENAME COLUMN price TO amount;
