-- ============================================================
-- 017_schema_alignment.sql — 全環境スキーマ整合性修正
-- ============================================================
-- 目的:
--   ローカル/ステージング/本番間のスキーマ差異を解消
--
-- 変更内容:
--   1. m_tax_rate.effective_to 追加（ローカルにのみ存在→全環境に展開）
--   2. 旧 t_billing_item テーブル削除（001で作成→007で再設計後の残骸）
-- ============================================================

-- 1. 税率マスタに適用終了日カラムを追加
ALTER TABLE m_tax_rate ADD COLUMN IF NOT EXISTS effective_to DATE;

-- 2. 旧テーブル削除（007_billing_redesign で t_billing に再設計済み）
DROP TABLE IF EXISTS t_billing_item CASCADE;
