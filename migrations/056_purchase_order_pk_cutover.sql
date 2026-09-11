-- 056_purchase_order_pk_cutover.sql
-- Phase D: t_purchase_order PK/FK 切替
-- 実行日: 2026-09-05

-- ============================================================
-- Phase D Step 1: order_id の UNIQUE 化（既にPKなので実質上の冗長性排除）
-- ============================================================
-- order_id は既に PRIMARY KEY なため、ここでの UNIQUE 制約追加は不要
-- （既存の PRIMARY KEY が UNIQUE を保証している）
-- Phase C の dual-write が完了していることを前提。

-- ============================================================
-- Phase D Step 2: 新 FK 制約を張る（phase C で dual-write 済みの前提）
-- ============================================================
-- ■ 注意: 両列が NOT NULL で埋まっていることを前提に進める。
-- ■      Phase C の dual-write が完全に適用されていない場合、以下は失敗する。
-- ■      失敗時は Phase C の完了を確認してから再実行すること。

-- t_purchase_order_item に新 FK 制約を追加
-- （旧 order_id FK との並行期間：下記ロールバック時まで両方維持）
ALTER TABLE t_purchase_order_item
    ADD CONSTRAINT fk_purchase_order_item_purchase_order_pk
    FOREIGN KEY (purchase_order_pk) REFERENCES t_purchase_order(id) ON DELETE CASCADE;

-- t_payment_notice に新 FK 制約を追加
ALTER TABLE t_payment_notice
    ADD CONSTRAINT fk_payment_notice_purchase_order_pk
    FOREIGN KEY (purchase_order_pk) REFERENCES t_purchase_order(id) ON DELETE CASCADE;

-- ============================================================
-- Phase D Step 3: 旧 FK 制約の削除（互換期間終了後。別途実施推奨）
-- ============================================================
-- ■ 本マイグレーションでは削除しない（フェーズ分離）。
-- ■ 旧制約の削除は以下を参照してから別チケットで実施：
--
--   ALTER TABLE t_purchase_order_item
--       DROP CONSTRAINT t_purchase_order_item_order_id_fkey;
--
--   ALTER TABLE t_payment_notice
--       DROP CONSTRAINT t_payment_notice_purchase_order_id_fkey;
--
--   -- 旧 order_id / purchase_order_id 列の削除はさらに後で
--   -- ALTER TABLE t_purchase_order_item DROP COLUMN order_id;
--   -- ALTER TABLE t_payment_notice DROP COLUMN purchase_order_id;
--
-- 理由:
--   1. 旧列削除は大規模テーブルの場合 EXCLUSIVE LOCK が長時間必要になる可能性
--   2. アプリ コード側の SELECT 一括置換が完了してから削除するべき
--   3. 緊急ロールバック時に旧列があると復旧が容易

-- ============================================================
-- Phase D Step 4: 検証
-- ============================================================
-- 実行後、以下を確認：
--
-- -- 両列が埋まっていることを確認
-- SELECT COUNT(*) FROM t_purchase_order_item WHERE purchase_order_pk IS NULL;  -- 結果: 0
-- SELECT COUNT(*) FROM t_payment_notice WHERE purchase_order_pk IS NULL;       -- 結果: 0
--
-- -- FK の参照性を確認（異常な孤児がないか）
-- SELECT COUNT(*) FROM t_purchase_order_item poi
--  WHERE NOT EXISTS (SELECT 1 FROM t_purchase_order po WHERE po.id = poi.purchase_order_pk);
-- -- 結果: 0
--
-- SELECT COUNT(*) FROM t_payment_notice pn
--  WHERE NOT EXISTS (SELECT 1 FROM t_purchase_order po WHERE po.id = pn.purchase_order_pk);
-- -- 結果: 0

-- ============================================================
-- Phase D Step 5: ロールバック（緊急時）
-- ============================================================
--
-- 新 FK を削除：
--   ALTER TABLE t_purchase_order_item
--       DROP CONSTRAINT fk_purchase_order_item_purchase_order_pk;
--   ALTER TABLE t_payment_notice
--       DROP CONSTRAINT fk_payment_notice_purchase_order_pk;
--
-- その後、アプリ側で Phase C のコード削除（SELECT/INSERT の新列参照を旧列に戻す）。
-- Phase C コード削除後、以下で新列を削除：
--   ALTER TABLE t_purchase_order_item DROP COLUMN purchase_order_pk;
--   ALTER TABLE t_payment_notice DROP COLUMN purchase_order_pk;
--   DROP INDEX idx_purchase_order_item_purchase_order_pk;
--   DROP INDEX idx_payment_notice_purchase_order_pk;
