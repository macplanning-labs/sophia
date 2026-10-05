-- ============================================================
-- 004_add_unique_constraints.sql — 重複防止制約
-- ============================================================
-- エンジニア名の表記ゆれ重複防止 + 発注書の二重作成防止
-- ============================================================

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- 1. m_engineer: 正規化名でのユニーク制約
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- スペース（半角・全角）・中黒を除去した正規化名カラムを追加
-- GENERATED ALWAYS AS ... STORED: DB側で自動計算・保存
-- アプリ側のINSERT/UPDATEコード変更は不要

ALTER TABLE m_engineer
  ADD COLUMN IF NOT EXISTS name_normalized VARCHAR(64)
  GENERATED ALWAYS AS (
    REPLACE(REPLACE(REPLACE(name, ' ', ''), '　', ''), '・', '')
  ) STORED;

-- アクティブなエンジニアの正規化名でユニーク制約
-- (is_active = false のエンジニアは除外)
CREATE UNIQUE INDEX IF NOT EXISTS idx_engineer_name_normalized_unique
  ON m_engineer (name_normalized)
  WHERE is_active = true;

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- 2. t_purchase_order: DRAFT状態の二重発注防止
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- 同一パートナー・プロジェクト・契約・期間のDRAFT発注書は1つまで

CREATE UNIQUE INDEX IF NOT EXISTS idx_purchase_order_unique_draft
  ON t_purchase_order (partner_id, project_id, partner_contract_id, work_start, work_end)
  WHERE status = 'DRAFT';
