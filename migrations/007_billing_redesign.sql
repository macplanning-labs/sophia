-- ============================================================
-- 007_billing_redesign.sql — 請求データモデル再設計
--
-- 旧テーブル（t_billing_item, t_billing_invoice）を DROP し、
-- 新テーブル（t_billing, t_billing_invoice）を作成する。
--
-- t_billing:          エンジニア × 月 × クライアント 単位の請求明細
-- t_billing_invoice:  クライアント × 月 単位の請求書ドキュメント
-- ============================================================

-- 1. 入金記録の外部キーを先に削除（t_billing_invoice を参照）
DROP TABLE IF EXISTS t_payment_record CASCADE;

-- 2. 既存テーブル DROP（FK 依存順序に注意）
DROP TABLE IF EXISTS t_billing_item CASCADE;
DROP TABLE IF EXISTS t_billing_invoice CASCADE;

-- 3. t_billing（請求: エンジニア × 月 × クライアント）
CREATE TABLE t_billing (
    id                  BIGSERIAL PRIMARY KEY,
    client_id           BIGINT NOT NULL REFERENCES m_client(id),
    target_month        DATE NOT NULL,
    engineer_id         BIGINT NOT NULL REFERENCES m_engineer(id),
    base_rate           INTEGER NOT NULL DEFAULT 0,
    settlement_type     VARCHAR(20) NOT NULL DEFAULT 'RANGE',
    lower_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 140.0,
    upper_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    deduction_rate      INTEGER NOT NULL DEFAULT 0,
    overtime_rate       INTEGER NOT NULL DEFAULT 0,
    effort              DECIMAL(3,2) NOT NULL DEFAULT 1.00,
    actual_hours        DECIMAL(6,2) NOT NULL DEFAULT 0,
    adjustment          INTEGER NOT NULL DEFAULT 0,
    amount              INTEGER NOT NULL DEFAULT 0,
    source              VARCHAR(10) NOT NULL DEFAULT 'SELF',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(client_id, target_month, engineer_id)
);

-- 4. t_billing_invoice（請求書: クライアント × 月 のドキュメント）
CREATE TABLE t_billing_invoice (
    id                  BIGSERIAL PRIMARY KEY,
    invoice_no          VARCHAR(20) NOT NULL UNIQUE,
    client_id           BIGINT NOT NULL REFERENCES m_client(id),
    target_month        DATE NOT NULL,
    work_start          DATE NOT NULL,
    work_end            DATE NOT NULL,
    issue_date          DATE NOT NULL DEFAULT CURRENT_DATE,
    due_date            DATE,
    subtotal            INTEGER NOT NULL DEFAULT 0,
    tax_amount          INTEGER NOT NULL DEFAULT 0,
    total               INTEGER NOT NULL DEFAULT 0,
    department          VARCHAR(128) NOT NULL DEFAULT '',
    subject             VARCHAR(255) NOT NULL DEFAULT '',
    registration_no     VARCHAR(30) NOT NULL DEFAULT '',
    edi_id              BIGINT,
    edi_invoice_no      VARCHAR(20) NOT NULL DEFAULT '',
    source              VARCHAR(10) NOT NULL DEFAULT 'SELF',
    status              VARCHAR(20) NOT NULL DEFAULT 'DRAFT',
    confirmed_at        TIMESTAMPTZ,
    pdf_file            VARCHAR(512) NOT NULL DEFAULT '',
    drive_file_id       VARCHAR(200) NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(client_id, target_month)
);

-- 5. 入金記録テーブルを再作成（新しい t_billing_invoice を参照）
CREATE TABLE t_payment_record (
    id              BIGSERIAL PRIMARY KEY,
    invoice_id      BIGINT NOT NULL REFERENCES t_billing_invoice(id) ON DELETE CASCADE,
    payment_date    DATE NOT NULL,
    amount          INTEGER NOT NULL,
    method          VARCHAR(10) NOT NULL DEFAULT 'TRANSFER',
    reference       VARCHAR(255) NOT NULL DEFAULT '',
    confirmed_by_id BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    confirmed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 6. インデックス
CREATE INDEX idx_billing_target_month ON t_billing(target_month);
CREATE INDEX idx_billing_client_month ON t_billing(client_id, target_month);
CREATE INDEX idx_billing_invoice_target_month ON t_billing_invoice(target_month);
