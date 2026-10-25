-- 077_mfa_totp_last_used_step.sql — 確認コード(TOTP)の使い回しを断る(DEMO-000153 S-1。SENN の A-3 と同じ)
--
-- 同じ確認コードは、有効な間(前後 1 区間 = 約 90 秒)に何度でも使えた。使えた最後の区間(UNIX 時間 / 30 秒)を持ち、
-- それ以前の区間のコードは断る。NULL は「まだ使っていない」。
ALTER TABLE s_totp_device ADD COLUMN IF NOT EXISTS last_used_step BIGINT;
