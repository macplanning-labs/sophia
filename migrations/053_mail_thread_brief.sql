-- Add support for mail thread brief cache and status tracking
-- Table for caching structured/Ollama summaries of mail threads

CREATE TABLE IF NOT EXISTS t_mail_thread_brief (
    from_email   VARCHAR(255) PRIMARY KEY,
    fingerprint  VARCHAR(64)  NOT NULL,
    summary_text TEXT         NOT NULL DEFAULT '',
    ollama_used  BOOLEAN      NOT NULL DEFAULT FALSE,
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- Add columns to t_received_email for manual review tracking
ALTER TABLE t_received_email ADD COLUMN IF NOT EXISTS needs_manual_review BOOLEAN NOT NULL DEFAULT FALSE;
