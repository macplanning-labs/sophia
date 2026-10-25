-- 078_mfa_user_attempt.sql — 二要素認証の試行の数え方を、ユーザー単位・DB 保存にする(DEMO-000153 S-1。SENN の A-1・A-2 と同じ)
--
-- これまでは「IP + ユーザー」の組・メモリ内で数えていたため、IP を変えれば数がゼロから始まり、再起動でも消えた。
-- ここは、**ユーザーごとに**数え、DB に保存する(複数台でも合算される。再起動で消えない)。
-- attempts は、時間窓(window_start から)の間に「確認を調べた回数」(確認コードとパスキーで合算)。
-- locked_until が未来なら、その人の二要素認証は止まっている。
CREATE TABLE IF NOT EXISTS s_mfa_user_attempt (
    user_id      BIGINT PRIMARY KEY,
    attempts     INTEGER NOT NULL DEFAULT 0,
    window_start TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    locked_until TIMESTAMPTZ
);

-- 期限切れの行を、古い順にたどって消すための索引(ロック中の行は除く)
CREATE INDEX IF NOT EXISTS idx_mfa_user_attempt_window
    ON s_mfa_user_attempt (window_start)
    WHERE locked_until IS NULL;
