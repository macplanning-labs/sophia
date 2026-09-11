-- メール自動取込パイプライン（4フェーズ疎結合設計）
--
-- Phase1(監視・分類) / Phase2(データソース取得) / Phase3(解析・登録) / Phase4(保存・通知)
-- 状態遷移: NEW → FETCHED → IMPORTED → (Drive保存完了で終端)
--           NEW → FETCH_FAILED / FETCHED → PARSE_FAILED → (retry_count>=3で needs_manual_review)
--           IMPORTED → DRIVE_FAILED（DB登録は成功、Drive保存のみ失敗。次回Phase4で再試行）

-- チェックポイント（Phase1: 未読/既読に依存しない差分取得の基点）
CREATE TABLE IF NOT EXISTS s_mail_sync_checkpoint (
    mailbox           VARCHAR(255) PRIMARY KEY,
    last_processed_at TIMESTAMPTZ NOT NULL,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- t_received_email 拡張（パイプライン状態管理用）
--
-- client_id: EDI連携先(m_client)からの注文/請求はpartner_idではなくclient_idで紐付ける。
--            Phase2がAPIポーリングで合成する行(message_id='edi-order:...'/'edi-invoice:...')でも使う。
-- raw_attachment: Phase2が取得した添付PDF/Excelの実バイナリ（Phase3がここから解析する）。
--            IMAPやEDI APIへの再アクセスなしにPhase3・Phase4を独立再実行できるようにするため、
--            ファイルパスではなくバイト列そのものをDBに保持する。
ALTER TABLE t_received_email
    ADD COLUMN IF NOT EXISTS source_type         VARCHAR(20)  NOT NULL DEFAULT 'UNKNOWN',
    ADD COLUMN IF NOT EXISTS client_id           BIGINT REFERENCES m_client(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS retry_count         INTEGER      NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS next_retry_at       TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS needs_manual_review BOOLEAN      NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS review_notified_at  TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS drive_file_id       VARCHAR(255) NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS drive_link          VARCHAR(512) NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS parsed_data         JSONB,
    ADD COLUMN IF NOT EXISTS raw_attachment       BYTEA;

CREATE INDEX IF NOT EXISTS idx_received_email_status ON t_received_email (status);
CREATE INDEX IF NOT EXISTS idx_received_email_review ON t_received_email (needs_manual_review) WHERE needs_manual_review = TRUE;
