-- 011_daily_work_entry.sql
-- 日次稼働報告テーブル + t_monthly_timesheet ロック機能

-- 日次稼働明細テーブル
CREATE TABLE IF NOT EXISTS t_daily_work_entry (
    id              BIGSERIAL PRIMARY KEY,
    engineer_id     BIGINT NOT NULL REFERENCES m_engineer(id) ON DELETE CASCADE,
    work_date       DATE NOT NULL,
    start_time      TIME,
    end_time        TIME,
    break_minutes   INTEGER NOT NULL DEFAULT 60,
    actual_minutes  INTEGER NOT NULL DEFAULT 0,
    work_description TEXT NOT NULL DEFAULT '',
    is_holiday      BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(engineer_id, work_date)
);

-- インデックス: 月単位の検索を高速化
CREATE INDEX IF NOT EXISTS idx_daily_work_entry_engineer_month
    ON t_daily_work_entry (engineer_id, work_date);

-- t_monthly_timesheet にロック機能追加（管理者がデータ確定した日時）
ALTER TABLE t_monthly_timesheet
    ADD COLUMN IF NOT EXISTS locked_at TIMESTAMPTZ;

COMMENT ON TABLE t_daily_work_entry IS '日次稼働報告。パートナー要員が日々の稼働を入力する。';
COMMENT ON COLUMN t_daily_work_entry.actual_minutes IS '実働時間（分）。(end_time - start_time) - break_minutes で自動計算。';
COMMENT ON COLUMN t_daily_work_entry.is_holiday IS '休日フラグ。trueの場合、実働時間は0。';
COMMENT ON COLUMN t_monthly_timesheet.locked_at IS '管理者がデータを確定した日時。NULLなら未確定（要員側から修正可能）。';
