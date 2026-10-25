-- 076_integration_daily_check.sql — 外部連携の毎朝の点検の記録(DEMO-000216)
-- 設計: docs/design/sophia_000210_integration_design.md 4章
--
-- - 毎朝 8:45(JST)の点検の結果を、1回につき1行残す。
-- - 同じ警告を毎日繰り返さないため、前回との違い(signature)を見て、変わったときだけ知らせる。
-- - 画面の上部に出す赤い帯も、直近の行を読む。
-- - 表を追加するだけ。既存の表は変えない。

CREATE TABLE IF NOT EXISTS h_integration_daily_check (
    id            BIGSERIAL PRIMARY KEY,
    run_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    ok            BOOLEAN     NOT NULL,
    problems      JSONB       NOT NULL DEFAULT '[]'::jsonb,  -- 問題の文(秘密の値は含めない)
    signature     TEXT        NOT NULL DEFAULT '',            -- 問題を並べ替えて連結したもの(空 = 問題なし)
    notified      BOOLEAN     NOT NULL DEFAULT FALSE,         -- この回に、管理者へ知らせたか(変わったときだけ true)
    notify_detail TEXT
);

CREATE INDEX IF NOT EXISTS idx_integration_daily_check_run_at ON h_integration_daily_check (run_at DESC);
