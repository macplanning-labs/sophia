-- 055_purchase_order_pk_surrogate.sql
-- Phase B: t_purchase_order をサロゲート PK へ移行（加算のみ）
-- 日付: 2026-09-04
--
-- 禁止: 旧 PK/FK の DROP、uuid 列の意味変更、本番未承認での破壊的切替
-- Phase C/D（dual-read・PK 切替）は別マイグレーションで実施

-- ============================================================
-- 1. t_purchase_order に新サロゲート列を追加
-- ============================================================
-- BIGSERIAL は既存行にも連番を埋める。DEFAULT に未作成シーケンス名を
-- 手書きすると失敗するため、BIGSERIAL のみで追加する。
ALTER TABLE t_purchase_order
    ADD COLUMN id BIGSERIAL;

ALTER TABLE t_purchase_order
    ADD CONSTRAINT t_purchase_order_id_unique UNIQUE (id);

COMMENT ON COLUMN t_purchase_order.id IS
    'サロゲートキー（§5.4）。業務IDは order_id。トークンURLは uuid。Phase B';

-- order_id PK は当面維持（互換期間）

-- ============================================================
-- 2. t_purchase_order_item に新 FK 候補列を追加
-- ============================================================
ALTER TABLE t_purchase_order_item
    ADD COLUMN purchase_order_pk BIGINT;

UPDATE t_purchase_order_item poi
SET purchase_order_pk = po.id
FROM t_purchase_order po
WHERE po.order_id = poi.order_id;

-- 孤児が残ると NOT NULL で失敗する（意図どおり・データ不整合の早期検知）
ALTER TABLE t_purchase_order_item
    ALTER COLUMN purchase_order_pk SET NOT NULL;

CREATE INDEX idx_purchase_order_item_purchase_order_pk
    ON t_purchase_order_item (purchase_order_pk);

COMMENT ON COLUMN t_purchase_order_item.purchase_order_pk IS
    't_purchase_order.id への将来FK候補。旧 order_id FK は互換期間維持。';

-- ============================================================
-- 3. t_payment_notice に新 FK 候補列を追加
-- ============================================================
ALTER TABLE t_payment_notice
    ADD COLUMN purchase_order_pk BIGINT;

UPDATE t_payment_notice pn
SET purchase_order_pk = po.id
FROM t_purchase_order po
WHERE po.order_id = pn.purchase_order_id;

ALTER TABLE t_payment_notice
    ALTER COLUMN purchase_order_pk SET NOT NULL;

CREATE INDEX idx_payment_notice_purchase_order_pk
    ON t_payment_notice (purchase_order_pk);

COMMENT ON COLUMN t_payment_notice.purchase_order_pk IS
    't_purchase_order.id への将来FK候補。旧 purchase_order_id FK は互換期間維持。';

-- ============================================================
-- 4. 旧 FK / PK は維持（Phase C/D で対応）
-- ============================================================
-- ■ t_purchase_order_item.order_id FK は削除しない
-- ■ t_payment_notice.purchase_order_id FK は削除しない
-- ■ order_id PK も削除しない
-- ■ 本マイグレーションでは FK CONSTRAINT もまだ張らない（アプリ dual-write 後）

-- ============================================================
-- 5. 検証（適用前後に実施）
-- ============================================================
-- SELECT COUNT(*) FROM t_purchase_order_item WHERE purchase_order_pk IS NULL;  -- 0
-- SELECT COUNT(*) FROM t_payment_notice WHERE purchase_order_pk IS NULL;       -- 0
-- SELECT COUNT(*) FROM t_purchase_order WHERE id IS NULL;                      -- 0
