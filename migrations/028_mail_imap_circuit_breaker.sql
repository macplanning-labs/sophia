-- IMAP認証エラー時のサーキットブレイカー用カラムを追加する。
-- 連続認証失敗(AUTHENTICATIONFAILED)が閾値に達したらIMAPアクセスを自動ロックし、
-- Googleへの誤った認証情報での再アクセス→再ブロックを防ぐ。ロック解除は管理者の手動操作のみ。
ALTER TABLE s_mail_sync_checkpoint
    ADD COLUMN IF NOT EXISTS consecutive_failures INT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS locked_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS lock_reason TEXT;
