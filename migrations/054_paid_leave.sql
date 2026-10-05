-- 043_paid_leave.sql — 年次有給休暇の管理（法定通り支給: 入社6ヶ月10日、以後毎年付与、繰越込み2年で時効）

-- 比例付与（パートタイム）判定用。NULLなら正社員の標準テーブルを適用
ALTER TABLE m_employee ADD COLUMN IF NOT EXISTS weekly_prescribed_days SMALLINT;

-- 付与バッチ（法定基準日ごとに1行。時効2年は expire_date で管理）
CREATE TABLE IF NOT EXISTS t_paid_leave_grant (
    id              BIGSERIAL PRIMARY KEY,
    employee_id     BIGINT NOT NULL REFERENCES m_employee(id) ON DELETE RESTRICT,
    grant_date      DATE NOT NULL,
    granted_days    DECIMAL(4,1) NOT NULL,
    expire_date     DATE NOT NULL,
    remaining_days  DECIMAL(4,1) NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (employee_id, grant_date)
);

CREATE INDEX IF NOT EXISTS idx_paid_leave_grant_employee ON t_paid_leave_grant(employee_id, expire_date);

-- 取得履歴（1申請=1行。半休0.5対応）
CREATE TABLE IF NOT EXISTS t_paid_leave_usage (
    id              BIGSERIAL PRIMARY KEY,
    employee_id     BIGINT NOT NULL REFERENCES m_employee(id) ON DELETE RESTRICT,
    used_date       DATE NOT NULL,
    days_used       DECIMAL(3,1) NOT NULL DEFAULT 1.0,
    year_month      DATE NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_paid_leave_usage_employee_month ON t_paid_leave_usage(employee_id, year_month);

-- 給与明細に確定時点の有給スナップショットを保持（他の支給・控除項目と同じ方式）
ALTER TABLE t_payroll ADD COLUMN IF NOT EXISTS paid_leave_used_days    DECIMAL(4,1) NOT NULL DEFAULT 0;
ALTER TABLE t_payroll ADD COLUMN IF NOT EXISTS paid_leave_balance_days DECIMAL(4,1) NOT NULL DEFAULT 0;
