-- 081_document_revision.sql — 送った書類の「訂正版」と「取り消し」(DEMO-000224)
--
-- 佐藤さんの方針(2026-10-13・10-19): 相手に送った書類は、削除も書き換えもしない。直したいときは、新しい版(訂正版)を作り、
-- 前の版は「差し替え済み」として残す。書類自体をなかったことにするときは「取り消し」にする(書類は消さず、理由を残す)。
-- 承諾済みの書類も同じ(承諾済みの版は、差し替え済みになっても残り、相手からずっと見える)。対象は請求書と支払通知書。
--
-- 列を足すだけ。既存の列・行は変えない(戻すときは、列を残したまま前のコードに戻せる)。
-- 079 の「送った書類の書き換え禁止」は、書類の内容の列だけを見ているので、ここで足す列は、送った後でも記録できる。
--
--   revision                版(1, 2, …)。訂正版を作るたびに、前の版の次の番号になる
--   supersedes_*            この版が差し替えた、前の版(最初の版は NULL)
--   revision_reason         訂正版を作った理由(最初の版は NULL)
--   cancelled_at / _by_id   取り消した日時・人
--   cancel_reason           取り消した理由
-- 「差し替え済み」は、列では持たず「この書類を supersedes に持つ版がある」ことで表す(送った書類を更新しないため)。

-- ── 支払通知書 ─────────────────────────────────────
ALTER TABLE t_payment_notice
    ADD COLUMN IF NOT EXISTS revision              INT         NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS supersedes_notice_id  VARCHAR     REFERENCES t_payment_notice (notice_id),
    ADD COLUMN IF NOT EXISTS revision_reason       TEXT,
    ADD COLUMN IF NOT EXISTS cancelled_at          TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS cancelled_by_id       BIGINT,
    ADD COLUMN IF NOT EXISTS cancel_reason         TEXT;

ALTER TABLE t_payment_notice DROP CONSTRAINT IF EXISTS ck_payment_notice_revision;
ALTER TABLE t_payment_notice ADD CONSTRAINT ck_payment_notice_revision CHECK (revision >= 1);

-- 1つの版を差し替える版は、1つだけ(同時に2つ作られないように)
CREATE UNIQUE INDEX IF NOT EXISTS uq_payment_notice_supersedes
    ON t_payment_notice (supersedes_notice_id) WHERE supersedes_notice_id IS NOT NULL;

-- ── 請求書 ─────────────────────────────────────────
ALTER TABLE t_billing_invoice
    ADD COLUMN IF NOT EXISTS revision              INT         NOT NULL DEFAULT 1,
    ADD COLUMN IF NOT EXISTS supersedes_invoice_id BIGINT      REFERENCES t_billing_invoice (id),
    ADD COLUMN IF NOT EXISTS revision_reason       TEXT,
    ADD COLUMN IF NOT EXISTS cancelled_at          TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS cancelled_by_id       BIGINT,
    ADD COLUMN IF NOT EXISTS cancel_reason         TEXT;

ALTER TABLE t_billing_invoice DROP CONSTRAINT IF EXISTS ck_billing_invoice_revision;
ALTER TABLE t_billing_invoice ADD CONSTRAINT ck_billing_invoice_revision CHECK (revision >= 1);

CREATE UNIQUE INDEX IF NOT EXISTS uq_billing_invoice_supersedes
    ON t_billing_invoice (supersedes_invoice_id) WHERE supersedes_invoice_id IS NOT NULL;

-- ── 取り消し・版の記録は、いったん付いたら変えさせない ─────────────
CREATE OR REPLACE FUNCTION guard_document_revision_record() RETURNS trigger AS $$
BEGIN
    IF OLD.cancelled_at IS NOT NULL AND (
           NEW.cancelled_at    IS DISTINCT FROM OLD.cancelled_at
        OR NEW.cancelled_by_id IS DISTINCT FROM OLD.cancelled_by_id
        OR NEW.cancel_reason   IS DISTINCT FROM OLD.cancel_reason
    ) THEN
        RAISE EXCEPTION '取り消した書類の、取り消しの記録は変更できません' USING ERRCODE = 'P0001';
    END IF;
    IF NEW.revision IS DISTINCT FROM OLD.revision
       OR NEW.revision_reason IS DISTINCT FROM OLD.revision_reason THEN
        RAISE EXCEPTION '書類の版の記録は変更できません' USING ERRCODE = 'P0001';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_payment_notice_revision ON t_payment_notice;
CREATE TRIGGER trg_guard_payment_notice_revision
    BEFORE UPDATE ON t_payment_notice
    FOR EACH ROW EXECUTE FUNCTION guard_document_revision_record();

DROP TRIGGER IF EXISTS trg_guard_billing_invoice_revision ON t_billing_invoice;
CREATE TRIGGER trg_guard_billing_invoice_revision
    BEFORE UPDATE ON t_billing_invoice
    FOR EACH ROW EXECUTE FUNCTION guard_document_revision_record();
