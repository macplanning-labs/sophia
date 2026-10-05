-- 058_attachment_reject.sql
-- 勤務表の添付を「差し戻し」できるようにする（勤務表の記載と、アプリの計算値が合わないときなど）。
-- 差し戻した勤務表は、取り込み待ちの一覧から外れ、理由と日時を残す。取り消して、取り込み待ちに戻せる。

ALTER TABLE t_received_email_attachment
    ADD COLUMN IF NOT EXISTS review_status   VARCHAR(16),
    ADD COLUMN IF NOT EXISTS rejected_reason TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS rejected_at     TIMESTAMPTZ;
