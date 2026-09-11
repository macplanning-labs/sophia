-- ============================================================
-- 018_expense_category_master.sql — 経費科目マスタ
-- ============================================================
-- これまでフロントエンドにハードコードされていた経費カテゴリ
-- (TRANSPORT/ENTERTAINMENT/SUPPLIES/COMMUNICATION/OTHER) をDB化。
-- マスタメンテ画面（/masters）から追加・編集・削除できるようにする。
-- t_expense_request.category には引き続き code(VARCHAR) を格納する
-- （スキーマ変更なし、既存データはそのまま有効）。

CREATE TABLE IF NOT EXISTS m_expense_category (
    id          BIGSERIAL PRIMARY KEY,
    code        VARCHAR(30)  NOT NULL UNIQUE,
    name        VARCHAR(50)  NOT NULL,
    sort_order  INT          NOT NULL DEFAULT 0,
    is_active   BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- 既存の運用で使われていたカテゴリを初期データとして投入
INSERT INTO m_expense_category (code, name, sort_order) VALUES
    ('TRANSPORT',      '交通費',   0),
    ('ENTERTAINMENT',  '交際費',   1),
    ('SUPPLIES',       '消耗品費', 2),
    ('COMMUNICATION',  '通信費',   3),
    ('OTHER',          'その他',   4)
ON CONFLICT (code) DO NOTHING;
