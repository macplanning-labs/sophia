-- ============================================================
-- 010_code_master.sql — コードマスタ（選択肢のDB化）
-- ============================================================
-- masters.rs にハードコーディングされていた選択肢をDBに移行。
-- category + code で一意。sort_order で表示順を制御。

CREATE TABLE IF NOT EXISTS m_code (
    id          SERIAL PRIMARY KEY,
    category    VARCHAR(50)  NOT NULL,  -- 選択肢カテゴリ（例: EDI_METHOD, SETTLEMENT_TYPE）
    code        VARCHAR(50)  NOT NULL,  -- 値（例: EDI_OASIS, RANGE）
    label       VARCHAR(100) NOT NULL,  -- 表示名（例: EDI-OASIS, 上下割）
    sort_order  INT          NOT NULL DEFAULT 0,
    is_active   BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE(category, code)
);

-- 初期データ投入
INSERT INTO m_code (category, code, label, sort_order) VALUES
    -- EDI方式
    ('EDI_METHOD', '',           'なし',         0),
    ('EDI_METHOD', 'EDI_OASIS', 'EDI-OASIS',    1),
    ('EDI_METHOD', 'EMAIL',     'メール',        2),
    -- 雇用形態
    ('EMPLOYMENT_TYPE', 'FULL_TIME', '正社員',    0),
    ('EMPLOYMENT_TYPE', 'CONTRACT',  '契約社員',  1),
    ('EMPLOYMENT_TYPE', 'PART_TIME', 'パート',    2),
    -- 口座種類
    ('ACCOUNT_TYPE', '普通', '普通', 0),
    ('ACCOUNT_TYPE', '当座', '当座', 1),
    -- 精算方式
    ('SETTLEMENT_TYPE', 'RANGE',            '上下割',     0),
    ('SETTLEMENT_TYPE', 'RANGE_MIDDLE',     '中間割',     1),
    ('SETTLEMENT_TYPE', 'RANGE_FIXED_RATE', '一律割',     2),
    ('SETTLEMENT_TYPE', 'FIXED',            '固定',       3),
    -- 月中ルール
    ('MID_MONTH_RULE', 'FIXED_HOURS', '固定時間制',           0),
    ('MID_MONTH_RULE', 'PRORATED',    '日割り計算',           1),
    ('MID_MONTH_RULE', 'FULL',        'フル（精算幅そのまま）', 2),
    ('MID_MONTH_RULE', 'HALF',        '半月',                 3),
    -- 所属区分
    ('AFFILIATION', 'PARTNER',  'パートナー', 0),
    ('AFFILIATION', 'EMPLOYEE', '自社社員',   1),
    -- 締め日
    ('CLOSING_DAY', '15', '15日締め', 0),
    ('CLOSING_DAY', '20', '20日締め', 1),
    ('CLOSING_DAY', '25', '25日締め', 2),
    ('CLOSING_DAY', '0',  '月末締め', 3),
    -- 支払月
    ('PAYMENT_MONTH', '0', '当月',   0),
    ('PAYMENT_MONTH', '1', '翌月',   1),
    ('PAYMENT_MONTH', '2', '翌々月', 2),
    -- 支払日
    ('PAYMENT_DAY', '10', '10日払い', 0),
    ('PAYMENT_DAY', '15', '15日払い', 1),
    ('PAYMENT_DAY', '20', '20日払い', 2),
    ('PAYMENT_DAY', '25', '25日払い', 3),
    ('PAYMENT_DAY', '0',  '末日払い', 4)
ON CONFLICT (category, code) DO NOTHING;
