-- 079_sent_document_immutable.sql — 相手に送った書類は、削除・書き換えをさせない(DEMO-000223)
--
-- 佐藤さんの方針(2026-10-13): 当社が発行して相手に確認を依頼した書類は、削除してはいけない。内容を変えたいときは、
-- 別の書類(新しい版)を作って、もう一度承認をもらう(何がダメだったかが分からなくならないように)。相手が承認した書類は、絶対に削除しない。
--
-- 画面・API でも止めている(domain/services/sent_document_lock.rs)が、抜け道があっても消せないよう、データベースの側でも拒む。
-- 「送った」= 請求書は sent_at、支払通知書は mail_sent_at が入っている。承諾の記録(client_accepted_at / partner_accepted_at)がある書類も、
-- 送ったものとして扱う。送る前の下書き・社内の承認の段階は、今までどおり削除・編集できる。
--
-- 拒むもの:
--   ・送った(承諾した)請求書・支払通知書の削除
--   ・送った請求書・支払通知書の「内容」(番号・取引先・金額・日付・件名・備考)の書き換え、明細の追加・変更・削除
--   ・送った(下書きでない)注文書の削除と、その明細の削除
-- 拒まないもの(送ったあとも起きる正常な更新): 状態・送信の記録(sent_*, mail_sent_at)・承諾の記録・照合値・PDF の置き場所・
--   担当(owner_*)・承認の記録・updated_at。
--
-- トリガーと関数を足すだけ。既存の表・列は変えない。

-- ── 請求書 ─────────────────────────────────────────
CREATE OR REPLACE FUNCTION guard_sent_invoice() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.sent_at IS NOT NULL OR OLD.client_accepted_at IS NOT NULL THEN
            RAISE EXCEPTION '相手に送った請求書は削除できません(invoice_no=%)', OLD.invoice_no
                USING ERRCODE = 'P0001';
        END IF;
        RETURN OLD;
    END IF;
    IF (OLD.sent_at IS NOT NULL OR OLD.client_accepted_at IS NOT NULL) AND (
           NEW.invoice_no       IS DISTINCT FROM OLD.invoice_no
        OR NEW.client_id        IS DISTINCT FROM OLD.client_id
        OR NEW.target_month     IS DISTINCT FROM OLD.target_month
        OR NEW.work_start       IS DISTINCT FROM OLD.work_start
        OR NEW.work_end         IS DISTINCT FROM OLD.work_end
        OR NEW.issue_date       IS DISTINCT FROM OLD.issue_date
        OR NEW.due_date         IS DISTINCT FROM OLD.due_date
        OR NEW.subtotal         IS DISTINCT FROM OLD.subtotal
        OR NEW.tax_amount       IS DISTINCT FROM OLD.tax_amount
        OR NEW.total            IS DISTINCT FROM OLD.total
        OR NEW.department       IS DISTINCT FROM OLD.department
        OR NEW.subject          IS DISTINCT FROM OLD.subject
        OR NEW.registration_no  IS DISTINCT FROM OLD.registration_no
        OR NEW.project_id       IS DISTINCT FROM OLD.project_id
        OR NEW.billing_unit     IS DISTINCT FROM OLD.billing_unit
        OR NEW.uuid             IS DISTINCT FROM OLD.uuid
    ) THEN
        RAISE EXCEPTION '相手に送った請求書は書き換えできません(invoice_no=%)', OLD.invoice_no
            USING ERRCODE = 'P0001';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_sent_invoice ON t_billing_invoice;
CREATE TRIGGER trg_guard_sent_invoice
    BEFORE UPDATE OR DELETE ON t_billing_invoice
    FOR EACH ROW EXECUTE FUNCTION guard_sent_invoice();

CREATE OR REPLACE FUNCTION guard_sent_invoice_item() RETURNS trigger AS $$
DECLARE
    locked boolean;
BEGIN
    -- 親の請求書が、送った(承諾した)ものか。親が無い(親の削除に伴う連鎖の削除)ときは、親の側の規則で守られている
    SELECT (sent_at IS NOT NULL OR client_accepted_at IS NOT NULL) INTO locked
      FROM t_billing_invoice WHERE id = COALESCE(NEW.invoice_id, OLD.invoice_id);
    IF COALESCE(locked, FALSE) THEN
        RAISE EXCEPTION '相手に送った請求書の明細は、追加・変更・削除できません'
            USING ERRCODE = 'P0001';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_sent_invoice_item ON t_billing_invoice_item;
CREATE TRIGGER trg_guard_sent_invoice_item
    BEFORE INSERT OR UPDATE OR DELETE ON t_billing_invoice_item
    FOR EACH ROW EXECUTE FUNCTION guard_sent_invoice_item();

-- ── 支払通知書 ─────────────────────────────────────
CREATE OR REPLACE FUNCTION guard_sent_payment_notice() RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.mail_sent_at IS NOT NULL OR OLD.partner_accepted_at IS NOT NULL THEN
            RAISE EXCEPTION '相手に送った支払通知書は削除できません(notice_id=%)', OLD.notice_id
                USING ERRCODE = 'P0001';
        END IF;
        RETURN OLD;
    END IF;
    IF (OLD.mail_sent_at IS NOT NULL OR OLD.partner_accepted_at IS NOT NULL) AND (
           NEW.notice_id          IS DISTINCT FROM OLD.notice_id
        OR NEW.uuid               IS DISTINCT FROM OLD.uuid
        OR NEW.purchase_order_id  IS DISTINCT FROM OLD.purchase_order_id
        OR NEW.purchase_order_pk  IS DISTINCT FROM OLD.purchase_order_pk
        OR NEW.partner_id         IS DISTINCT FROM OLD.partner_id
        OR NEW.target_month       IS DISTINCT FROM OLD.target_month
        OR NEW.notice_date        IS DISTINCT FROM OLD.notice_date
        OR NEW.payment_due_date   IS DISTINCT FROM OLD.payment_due_date
        OR NEW.subtotal           IS DISTINCT FROM OLD.subtotal
        OR NEW.tax_amount         IS DISTINCT FROM OLD.tax_amount
        OR NEW.total              IS DISTINCT FROM OLD.total
        OR NEW.remarks            IS DISTINCT FROM OLD.remarks
    ) THEN
        RAISE EXCEPTION '相手に送った支払通知書は書き換えできません(notice_id=%)', OLD.notice_id
            USING ERRCODE = 'P0001';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_sent_payment_notice ON t_payment_notice;
CREATE TRIGGER trg_guard_sent_payment_notice
    BEFORE UPDATE OR DELETE ON t_payment_notice
    FOR EACH ROW EXECUTE FUNCTION guard_sent_payment_notice();

CREATE OR REPLACE FUNCTION guard_sent_payment_notice_item() RETURNS trigger AS $$
DECLARE
    locked boolean;
BEGIN
    SELECT (mail_sent_at IS NOT NULL OR partner_accepted_at IS NOT NULL) INTO locked
      FROM t_payment_notice WHERE notice_id = COALESCE(NEW.notice_id, OLD.notice_id);
    IF COALESCE(locked, FALSE) THEN
        RAISE EXCEPTION '相手に送った支払通知書の明細は、追加・変更・削除できません'
            USING ERRCODE = 'P0001';
    END IF;
    IF TG_OP = 'DELETE' THEN
        RETURN OLD;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_sent_payment_notice_item ON t_payment_notice_item;
CREATE TRIGGER trg_guard_sent_payment_notice_item
    BEFORE INSERT OR UPDATE OR DELETE ON t_payment_notice_item
    FOR EACH ROW EXECUTE FUNCTION guard_sent_payment_notice_item();

-- ── 注文書(削除だけ。送った=下書き以外。編集は、アプリが下書きのときだけに限っている) ──
CREATE OR REPLACE FUNCTION guard_sent_purchase_order() RETURNS trigger AS $$
BEGIN
    IF OLD.status <> 'DRAFT' THEN
        RAISE EXCEPTION '相手に送った注文書は削除できません(order_id=%)', OLD.order_id
            USING ERRCODE = 'P0001';
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_sent_purchase_order ON t_purchase_order;
CREATE TRIGGER trg_guard_sent_purchase_order
    BEFORE DELETE ON t_purchase_order
    FOR EACH ROW EXECUTE FUNCTION guard_sent_purchase_order();

CREATE OR REPLACE FUNCTION guard_sent_purchase_order_item() RETURNS trigger AS $$
DECLARE
    st text;
BEGIN
    SELECT status INTO st FROM t_purchase_order WHERE order_id = OLD.order_id;
    IF st IS NOT NULL AND st <> 'DRAFT' THEN
        RAISE EXCEPTION '相手に送った注文書の明細は削除できません(order_id=%)', OLD.order_id
            USING ERRCODE = 'P0001';
    END IF;
    RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_guard_sent_purchase_order_item ON t_purchase_order_item;
CREATE TRIGGER trg_guard_sent_purchase_order_item
    BEFORE DELETE ON t_purchase_order_item
    FOR EACH ROW EXECUTE FUNCTION guard_sent_purchase_order_item();
