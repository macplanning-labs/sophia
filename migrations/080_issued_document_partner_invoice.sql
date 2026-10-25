-- 080_issued_document_partner_invoice.sql — 発行した書類の種類に「代理請求書」を足す(DEMO-000087)
--
-- 支払通知書をパートナーが承諾した時点の代理請求書 PDF を、コンテナ内の uploads/invoices ではなく、
-- 発行した書類の記録(t_issued_document)に残す。種類の許可リストに 'partner_invoice' を足すだけ。
-- 既存の行・列は変えない(戻すときは、表を残したまま前のコードに戻せる)。

ALTER TABLE t_issued_document DROP CONSTRAINT IF EXISTS ck_issued_document_doc_type;
ALTER TABLE t_issued_document ADD CONSTRAINT ck_issued_document_doc_type
    CHECK (doc_type IN ('purchase_order', 'payment_notice', 'invoice', 'acceptance', 'partner_invoice'));
