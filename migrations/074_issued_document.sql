-- 074_issued_document.sql — 発行した書類の PDF を、送った瞬間のまま残す(DEMO-000214)
-- 設計: docs/design/sophia_000210_integration_design.md 5章(D2・D6・D7)
--
-- - 注文書・支払通知書・請求書を相手に送った瞬間に PDF を1回だけ作り、ここに残す。相手がリンクから開いたときも、これを見せる。
-- - PDF の本体は別の表(t_issued_document_content)に置く。一覧・検索で大きなデータを読まないため。
-- - 行は更新・削除しない(電子帳簿保存法の「消したり直したりできない」への備え)。更新してよいのは Drive への保存の状態の列だけ。
--   トリガーで、本体の更新・削除と、書類を表す列の更新を拒む。
-- - 表を追加するだけ。既存の表は変えない(戻すときは、表を残したまま前のコードに戻せる)。

CREATE TABLE IF NOT EXISTS t_issued_document (
    id              BIGSERIAL PRIMARY KEY,
    doc_type        TEXT        NOT NULL,   -- 'purchase_order' | 'payment_notice' | 'invoice' | 'acceptance'
    doc_ref         TEXT        NOT NULL,   -- 書類の番号(注文書ID・支払通知書ID・請求書番号)
    version         INT         NOT NULL,   -- 1, 2, …(内容を変えて送り直すと増える)
    party_kind      TEXT        NOT NULL,   -- 'partner' | 'client'
    party_name      TEXT        NOT NULL,   -- 送った時点の取引先名(Drive のフォルダ名にも使う)
    target_month    DATE,                   -- 対象年月(その月の1日)
    amount          BIGINT,                 -- 税込金額(探すため。無い書類は NULL)
    file_name       TEXT        NOT NULL,   -- <書類番号>_<対象年月>.pdf(版は Drive に保存するときに _v2 … を付ける)
    sha256          TEXT        NOT NULL,
    size_bytes      INT         NOT NULL,
    sent_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    store_status    TEXT        NOT NULL DEFAULT 'pending',  -- 'pending' | 'stored'
    store_attempts  INT         NOT NULL DEFAULT 0,
    store_last_error TEXT,
    store_file_id   TEXT,
    store_file_name TEXT,                   -- Drive に実際に付いた名前(_v2 などを含む)
    store_url       TEXT,
    stored_at       TIMESTAMPTZ,
    CONSTRAINT uq_issued_document_version UNIQUE (doc_type, doc_ref, version),
    CONSTRAINT ck_issued_document_doc_type CHECK (doc_type IN ('purchase_order', 'payment_notice', 'invoice', 'acceptance')),
    CONSTRAINT ck_issued_document_party_kind CHECK (party_kind IN ('partner', 'client')),
    CONSTRAINT ck_issued_document_store_status CHECK (store_status IN ('pending', 'stored'))
);

CREATE INDEX IF NOT EXISTS idx_issued_document_pending ON t_issued_document (id) WHERE store_status = 'pending';
CREATE INDEX IF NOT EXISTS idx_issued_document_sent_at ON t_issued_document (sent_at);

COMMENT ON TABLE t_issued_document IS '発行した書類(送った瞬間の PDF)の記録。行は更新・削除しない(保存の状態の列を除く)';

CREATE TABLE IF NOT EXISTS t_issued_document_content (
    issued_document_id BIGINT PRIMARY KEY REFERENCES t_issued_document (id),
    content            BYTEA NOT NULL
);

COMMENT ON TABLE t_issued_document_content IS '発行した書類の PDF の本体。更新・削除しない';

-- 本体は更新・削除させない
CREATE OR REPLACE FUNCTION f_issued_document_content_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION '発行した書類の PDF は、更新・削除できません(t_issued_document_content)';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_issued_document_content_immutable ON t_issued_document_content;
CREATE TRIGGER trg_issued_document_content_immutable
    BEFORE UPDATE OR DELETE ON t_issued_document_content
    FOR EACH ROW EXECUTE FUNCTION f_issued_document_content_immutable();

-- 記録の行は削除させない。更新は保存の状態の列だけ
CREATE OR REPLACE FUNCTION f_issued_document_guard() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION '発行した書類の記録は、削除できません(t_issued_document)';
    END IF;
    IF NEW.doc_type IS DISTINCT FROM OLD.doc_type
       OR NEW.doc_ref IS DISTINCT FROM OLD.doc_ref
       OR NEW.version IS DISTINCT FROM OLD.version
       OR NEW.party_kind IS DISTINCT FROM OLD.party_kind
       OR NEW.party_name IS DISTINCT FROM OLD.party_name
       OR NEW.target_month IS DISTINCT FROM OLD.target_month
       OR NEW.amount IS DISTINCT FROM OLD.amount
       OR NEW.file_name IS DISTINCT FROM OLD.file_name
       OR NEW.sha256 IS DISTINCT FROM OLD.sha256
       OR NEW.size_bytes IS DISTINCT FROM OLD.size_bytes
       OR NEW.sent_at IS DISTINCT FROM OLD.sent_at THEN
        RAISE EXCEPTION '発行した書類の記録は、保存の状態の列のほかは変更できません(t_issued_document)';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_issued_document_guard ON t_issued_document;
CREATE TRIGGER trg_issued_document_guard
    BEFORE UPDATE OR DELETE ON t_issued_document
    FOR EACH ROW EXECUTE FUNCTION f_issued_document_guard();
