-- 064_edi_login_log.sql
-- 取引先の EDI システムへのログインを、きっかけの通知メールとともに1回ずつ記録する（DEMO-000148）。
-- ログインしてよいのは、取引先から要対応の通知メールが届いたときだけ。
-- 「なぜログインしたか」を、received_email_ids（きっかけの受信メールの id）で後から確かめられるようにする。
CREATE TABLE IF NOT EXISTS t_edi_login_log (
    id                 BIGSERIAL    PRIMARY KEY,
    logged_in_at       TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    received_email_ids BIGINT[]     NOT NULL CHECK (cardinality(received_email_ids) > 0),
    success            BOOLEAN      NOT NULL,
    error_message      TEXT         NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_edi_login_log_logged_in_at ON t_edi_login_log (logged_in_at DESC);
