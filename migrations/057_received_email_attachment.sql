-- 057_received_email_attachment.sql
-- 受信メールの添付を1件ずつ保存する（1通に複数名分の勤務表が添付されるケース対応）
-- 従来は t_received_email.raw_attachment に「最初の1件」しか保存しておらず、
-- 2件目以降は取りこぼしていた。t_received_email.raw_attachment は互換のため残し、
-- 受注/請求書PDFの取込（Phase3）は従来どおりそちらを使う。

CREATE TABLE IF NOT EXISTS t_received_email_attachment (
    id                    BIGSERIAL PRIMARY KEY,
    email_id              BIGINT       NOT NULL REFERENCES t_received_email(id) ON DELETE CASCADE,
    seq                   INTEGER      NOT NULL,
    filename              VARCHAR(512) NOT NULL,
    content_type          VARCHAR(128) NOT NULL DEFAULT '',
    content               BYTEA        NOT NULL,
    -- 勤務表として読み取った結果（初回の参照時にキャッシュ）
    parsed_json           JSONB,
    -- 稼働報告に取り込み済みのとき、その稼働報告ID
    imported_timesheet_id BIGINT       REFERENCES t_monthly_timesheet(id) ON DELETE SET NULL,
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (email_id, seq)
);

CREATE INDEX IF NOT EXISTS idx_received_email_attachment_email
    ON t_received_email_attachment (email_id);

-- 既存メールの添付（最初の1件）のうち、勤務表として読める拡張子のものを引き継ぐ
INSERT INTO t_received_email_attachment (email_id, seq, filename, content, created_at)
SELECT id, 0, attachment_filename, raw_attachment, created_at
FROM t_received_email
WHERE raw_attachment IS NOT NULL
  AND source_type = 'ATTACHMENT'
  AND lower(attachment_filename) ~ '\.(pdf|xlsx|xlsm|xls)$'
ON CONFLICT (email_id, seq) DO NOTHING;

-- 承認済みの稼働報告と同名ファイルの添付は、取り込み済みとして結び付けておく。
-- 承認前（提出済・差戻し等）のものは、人が内容を確認して取り込む前提なので、未取込のまま残す。
UPDATE t_received_email_attachment a
SET imported_timesheet_id = (
    SELECT MAX(t.id) FROM t_monthly_timesheet t
    WHERE t.original_filename = a.filename AND t.status = 'APPROVED'
)
WHERE a.imported_timesheet_id IS NULL;
