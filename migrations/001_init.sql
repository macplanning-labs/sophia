-- ============================================================
-- 001_init.sql — Sophia 全テーブル定義
-- ============================================================
-- 概要設計書 §5 準拠。Django版と同一の db_table 名を維持。
-- プレフィックス: s_(システム), m_(マスタ), t_(トランザクション), h_(履歴)
-- ============================================================

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- システム (s_)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- ユーザー（認証用）
CREATE TABLE IF NOT EXISTS s_user (
    id          BIGSERIAL PRIMARY KEY,
    email       VARCHAR(255) NOT NULL UNIQUE,
    password    VARCHAR(255) NOT NULL,
    username    VARCHAR(150) NOT NULL,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    is_staff    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- セッション管理
CREATE TABLE IF NOT EXISTS s_session (
    session_id  VARCHAR(64) PRIMARY KEY,
    user_id     BIGINT NOT NULL REFERENCES s_user(id) ON DELETE CASCADE,
    role        VARCHAR(20) NOT NULL DEFAULT 'USER',
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_session_user ON s_session(user_id);
CREATE INDEX IF NOT EXISTS idx_session_expires ON s_session(expires_at);

-- 自社情報
CREATE TABLE IF NOT EXISTS s_company_info (
    id                      BIGSERIAL PRIMARY KEY,
    name                    VARCHAR(128) NOT NULL DEFAULT '',
    postal_code             VARCHAR(10) NOT NULL DEFAULT '',
    address                 VARCHAR(255) NOT NULL DEFAULT '',
    tel                     VARCHAR(20) NOT NULL DEFAULT '',
    fax                     VARCHAR(20) NOT NULL DEFAULT '',
    representative_title    VARCHAR(64) NOT NULL DEFAULT '代表取締役',
    representative_name     VARCHAR(64) NOT NULL DEFAULT '',
    registration_no         VARCHAR(20) NOT NULL DEFAULT '',
    responsible_person      VARCHAR(64) NOT NULL DEFAULT '',
    contact_person          VARCHAR(64) NOT NULL DEFAULT '',
    bank_name               VARCHAR(64) NOT NULL DEFAULT '',
    bank_branch             VARCHAR(64) NOT NULL DEFAULT '',
    account_type            VARCHAR(20) NOT NULL DEFAULT '普通',
    account_number          VARCHAR(20) NOT NULL DEFAULT '',
    account_name            VARCHAR(128) NOT NULL DEFAULT '',
    stamp_image             VARCHAR(512) NOT NULL DEFAULT '',
    logo_image              VARCHAR(512) NOT NULL DEFAULT '',
    tax_rate                DECIMAL(5,2) NOT NULL DEFAULT 10.00,
    email_host              VARCHAR(255) NOT NULL DEFAULT '',
    email_port              INTEGER,
    email_use_tls           BOOLEAN NOT NULL DEFAULT TRUE,
    email_host_user         VARCHAR(255) NOT NULL DEFAULT '',
    email_host_password     VARCHAR(255) NOT NULL DEFAULT '',
    default_from_email      VARCHAR(255) NOT NULL DEFAULT ''
);

-- メールテンプレート
CREATE TABLE IF NOT EXISTS s_email_template (
    id          BIGSERIAL PRIMARY KEY,
    code        VARCHAR(50) NOT NULL UNIQUE,
    subject     VARCHAR(255) NOT NULL,
    body        TEXT NOT NULL,
    description VARCHAR(255) NOT NULL DEFAULT '',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ユーザープロフィール
CREATE TABLE IF NOT EXISTS s_user_profile (
    id              BIGSERIAL PRIMARY KEY,
    user_id         BIGINT NOT NULL UNIQUE REFERENCES s_user(id) ON DELETE CASCADE,
    partner_id      VARCHAR(32),
    is_first_login  BOOLEAN NOT NULL DEFAULT TRUE
);

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- マスタ (m_)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- クライアント
CREATE TABLE IF NOT EXISTS m_client (
    id                      BIGSERIAL PRIMARY KEY,
    name                    VARCHAR(128) NOT NULL,
    contact_person          VARCHAR(64) NOT NULL DEFAULT '',
    email                   VARCHAR(255) NOT NULL DEFAULT '',
    cc_email                TEXT NOT NULL DEFAULT '',
    phone                   VARCHAR(20) NOT NULL DEFAULT '',
    postal_code             VARCHAR(10) NOT NULL DEFAULT '',
    address                 VARCHAR(255) NOT NULL DEFAULT '',
    representative_name     VARCHAR(64) NOT NULL DEFAULT '',
    representative_title    VARCHAR(64) NOT NULL DEFAULT '',
    registration_no         VARCHAR(20) NOT NULL DEFAULT '',
    url                     VARCHAR(200) NOT NULL DEFAULT '',
    report_email            TEXT NOT NULL DEFAULT '',
    work_report_email       VARCHAR(255) NOT NULL DEFAULT '',
    invoice_email           VARCHAR(255) NOT NULL DEFAULT '',
    edi_system_type         VARCHAR(20) NOT NULL DEFAULT '',
    edi_notification_email  VARCHAR(255) NOT NULL DEFAULT ''
);

-- パートナー
CREATE TABLE IF NOT EXISTS m_partner (
    partner_id                  VARCHAR(32) PRIMARY KEY,
    name                        VARCHAR(128) NOT NULL,
    name_kana                   VARCHAR(255) NOT NULL DEFAULT '',
    postal_code                 VARCHAR(10) NOT NULL DEFAULT '',
    address                     VARCHAR(255) NOT NULL DEFAULT '',
    tel                         VARCHAR(20) NOT NULL DEFAULT '',
    fax                         VARCHAR(20) NOT NULL DEFAULT '',
    email                       VARCHAR(255) NOT NULL,
    report_email                VARCHAR(255) NOT NULL DEFAULT '',
    cc                          TEXT NOT NULL DEFAULT '',
    bcc                         TEXT NOT NULL DEFAULT '',
    representative_name         VARCHAR(64) NOT NULL DEFAULT '',
    representative_name_kana    VARCHAR(128) NOT NULL DEFAULT '',
    representative_position     VARCHAR(64) NOT NULL DEFAULT '',
    responsible_person          VARCHAR(64) NOT NULL DEFAULT '',
    contact_person              VARCHAR(64) NOT NULL DEFAULT '',
    registration_no             VARCHAR(20) NOT NULL DEFAULT '',
    staff_contact_id            BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    attachment_file             VARCHAR(512) NOT NULL DEFAULT '',
    bank_name                   VARCHAR(64) NOT NULL DEFAULT '',
    bank_branch                 VARCHAR(64) NOT NULL DEFAULT '',
    account_type                VARCHAR(20) NOT NULL DEFAULT '普通',
    account_number              VARCHAR(20) NOT NULL DEFAULT '',
    account_name                VARCHAR(128) NOT NULL DEFAULT ''
);

-- 案件
CREATE TABLE IF NOT EXISTS m_project (
    project_id  VARCHAR(32) PRIMARY KEY,
    client_id   BIGINT NOT NULL REFERENCES m_client(id) ON DELETE RESTRICT,
    name        VARCHAR(200) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- エンジニア
CREATE TABLE IF NOT EXISTS m_engineer (
    id              BIGSERIAL PRIMARY KEY,
    name            VARCHAR(64) NOT NULL,
    name_kana       VARCHAR(128) NOT NULL DEFAULT '',
    affiliation_type VARCHAR(10) NOT NULL DEFAULT 'PARTNER',
    partner_id      VARCHAR(32) REFERENCES m_partner(partner_id) ON DELETE SET NULL,
    employee_id     VARCHAR(20) NOT NULL DEFAULT '',
    email           VARCHAR(255) NOT NULL DEFAULT '',
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 勤務場所
CREATE TABLE IF NOT EXISTS m_workplace (
    id      BIGSERIAL PRIMARY KEY,
    name    VARCHAR(128) NOT NULL,
    address VARCHAR(255) NOT NULL DEFAULT ''
);

-- 支払条件
CREATE TABLE IF NOT EXISTS m_payment_term (
    id                      BIGSERIAL PRIMARY KEY,
    partner_id              VARCHAR(32) NOT NULL REFERENCES m_partner(partner_id) ON DELETE CASCADE,
    client_id               BIGINT REFERENCES m_client(id) ON DELETE CASCADE,
    description             TEXT NOT NULL DEFAULT '',
    closing_day             INTEGER NOT NULL DEFAULT 31,
    payment_month_offset    INTEGER NOT NULL DEFAULT 1,
    payment_day             INTEGER NOT NULL DEFAULT 31
);

-- 銀行マスタ
CREATE TABLE IF NOT EXISTS m_bank_master (
    id          BIGSERIAL PRIMARY KEY,
    bank_code   VARCHAR(4) NOT NULL,
    bank_name   VARCHAR(128) NOT NULL,
    branch_code VARCHAR(3) NOT NULL,
    branch_name VARCHAR(128) NOT NULL,
    UNIQUE (bank_code, branch_code)
);

-- 基本契約進捗
CREATE TABLE IF NOT EXISTS m_contract_progress (
    id              BIGSERIAL PRIMARY KEY,
    partner_id      VARCHAR(32) NOT NULL UNIQUE REFERENCES m_partner(partner_id) ON DELETE CASCADE,
    status          VARCHAR(20) NOT NULL DEFAULT 'INVITED',
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    contract_pdf    VARCHAR(512) NOT NULL DEFAULT '',
    pdf_hash        VARCHAR(64) NOT NULL DEFAULT '',
    sent_at         TIMESTAMPTZ,
    signed_at       TIMESTAMPTZ,
    signed_by_id    BIGINT REFERENCES s_user(id) ON DELETE SET NULL
);

-- パートナー契約（発注マスタ）— 精算条件フィールド含む
CREATE TABLE IF NOT EXISTS m_partner_contract (
    id                      BIGSERIAL PRIMARY KEY,
    project_id              VARCHAR(32) NOT NULL REFERENCES m_project(project_id) ON DELETE RESTRICT,
    engineer_id             BIGINT NOT NULL REFERENCES m_engineer(id) ON DELETE RESTRICT,
    partner_id              VARCHAR(32) NOT NULL REFERENCES m_partner(partner_id) ON DELETE RESTRICT,
    start_date              DATE NOT NULL,
    end_date                DATE NOT NULL,
    -- 精算条件（SettlementFields）
    settlement_type         VARCHAR(10) NOT NULL DEFAULT 'RANGE',
    lower_limit_hours       DECIMAL(5,1) NOT NULL DEFAULT 140.0,
    upper_limit_hours       DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    fixed_hours             DECIMAL(5,1),
    base_rate               INTEGER NOT NULL DEFAULT 0,
    deduction_rate          INTEGER NOT NULL DEFAULT 0,
    overtime_rate           INTEGER NOT NULL DEFAULT 0,
    effort                  DECIMAL(3,2) NOT NULL DEFAULT 1.00,
    mid_month_rule          VARCHAR(10) NOT NULL DEFAULT 'FULL',
    -- 注文書テンプレート情報
    甲_責任者               VARCHAR(64) NOT NULL DEFAULT '',
    甲_担当者               VARCHAR(64) NOT NULL DEFAULT '',
    乙_責任者               VARCHAR(64) NOT NULL DEFAULT '',
    乙_担当者               VARCHAR(64) NOT NULL DEFAULT '',
    作業責任者               VARCHAR(64) NOT NULL DEFAULT '',
    workplace_id            BIGINT REFERENCES m_workplace(id) ON DELETE SET NULL,
    work_location           VARCHAR(255) NOT NULL DEFAULT '',
    deliverable_text        VARCHAR(255) NOT NULL DEFAULT '月別作業報告書',
    payment_condition       TEXT NOT NULL DEFAULT '毎月末日締め翌月末日払い（税別）',
    contract_items          TEXT NOT NULL DEFAULT '',
    currency                VARCHAR(3) NOT NULL DEFAULT 'JPY',
    reminder_days_before    INTEGER NOT NULL DEFAULT 3,
    alert_days_after        INTEGER NOT NULL DEFAULT 3,
    remarks                 TEXT NOT NULL DEFAULT '',
    is_active               BOOLEAN NOT NULL DEFAULT TRUE,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, engineer_id, start_date)
);

-- 顧客契約（受注マスタ）— 精算条件フィールド含む
CREATE TABLE IF NOT EXISTS m_client_contract (
    id                          BIGSERIAL PRIMARY KEY,
    project_id                  VARCHAR(32) NOT NULL REFERENCES m_project(project_id) ON DELETE RESTRICT,
    engineer_id                 BIGINT NOT NULL REFERENCES m_engineer(id) ON DELETE RESTRICT,
    start_date                  DATE NOT NULL,
    end_date                    DATE NOT NULL,
    -- 精算条件（SettlementFields）
    settlement_type             VARCHAR(10) NOT NULL DEFAULT 'RANGE',
    lower_limit_hours           DECIMAL(5,1) NOT NULL DEFAULT 140.0,
    upper_limit_hours           DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    fixed_hours                 DECIMAL(5,1),
    base_rate                   INTEGER NOT NULL DEFAULT 0,
    deduction_rate              INTEGER NOT NULL DEFAULT 0,
    overtime_rate               INTEGER NOT NULL DEFAULT 0,
    effort                      DECIMAL(3,2) NOT NULL DEFAULT 1.00,
    mid_month_rule              VARCHAR(10) NOT NULL DEFAULT 'FULL',
    -- 請求情報
    billing_timing              VARCHAR(20) NOT NULL DEFAULT 'LAST_DAY',
    payment_terms               VARCHAR(255) NOT NULL DEFAULT '毎月末日締め翌月末日払い',
    report_deadline_days_before INTEGER NOT NULL DEFAULT 1,
    currency                    VARCHAR(3) NOT NULL DEFAULT 'JPY',
    remarks                     TEXT NOT NULL DEFAULT '',
    is_active                   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, engineer_id, start_date)
);

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- 給与マスタ (m_)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- 社員
CREATE TABLE IF NOT EXISTS m_employee (
    id                      BIGSERIAL PRIMARY KEY,
    employee_id             VARCHAR(20) NOT NULL UNIQUE,
    last_name               VARCHAR(32) NOT NULL,
    first_name              VARCHAR(32) NOT NULL,
    last_name_kana          VARCHAR(64) NOT NULL DEFAULT '',
    first_name_kana         VARCHAR(64) NOT NULL DEFAULT '',
    employment_type         VARCHAR(10) NOT NULL DEFAULT 'REGULAR',
    birth_date              DATE,
    hire_date               DATE,
    email                   VARCHAR(255) NOT NULL DEFAULT '',
    base_salary             INTEGER NOT NULL DEFAULT 0,
    position_allowance      INTEGER NOT NULL DEFAULT 0,
    housing_allowance       INTEGER NOT NULL DEFAULT 0,
    commuting_allowance     INTEGER NOT NULL DEFAULT 0,
    standard_monthly_hours  DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    standard_remuneration   INTEGER NOT NULL DEFAULT 0,
    insurance_start_date    DATE,
    dependents_count        INTEGER NOT NULL DEFAULT 0,
    is_tax_exempt           BOOLEAN NOT NULL DEFAULT FALSE,
    pension_enrolled        BOOLEAN NOT NULL DEFAULT TRUE,
    health_enrolled         BOOLEAN NOT NULL DEFAULT TRUE,
    nursing_enrolled        BOOLEAN NOT NULL DEFAULT FALSE,
    employment_enrolled     BOOLEAN NOT NULL DEFAULT TRUE,
    is_active               BOOLEAN NOT NULL DEFAULT TRUE,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 社員振込先
CREATE TABLE IF NOT EXISTS m_employee_bank_account (
    id                  BIGSERIAL PRIMARY KEY,
    employee_id         BIGINT NOT NULL UNIQUE REFERENCES m_employee(id) ON DELETE CASCADE,
    bank_name           VARCHAR(64) NOT NULL,
    bank_code           VARCHAR(4) NOT NULL DEFAULT '',
    branch_name         VARCHAR(64) NOT NULL,
    branch_code         VARCHAR(10) NOT NULL DEFAULT '',
    account_type        VARCHAR(10) NOT NULL DEFAULT 'ordinary',
    account_number      VARCHAR(20) NOT NULL,
    account_holder_kana VARCHAR(128) NOT NULL
);

-- 保険料率
CREATE TABLE IF NOT EXISTS m_insurance_rate (
    id                          BIGSERIAL PRIMARY KEY,
    fiscal_year                 INTEGER NOT NULL UNIQUE,
    pension_rate                DECIMAL(5,2) NOT NULL DEFAULT 18.30,
    health_rate                 DECIMAL(5,2) NOT NULL DEFAULT 9.98,
    nursing_rate                DECIMAL(5,2) NOT NULL DEFAULT 1.60,
    employment_rate_employee    DECIMAL(5,3) NOT NULL DEFAULT 0.600
);

-- 源泉徴収税額表
CREATE TABLE IF NOT EXISTS m_withholding_tax (
    id          BIGSERIAL PRIMARY KEY,
    fiscal_year INTEGER NOT NULL,
    salary_from INTEGER NOT NULL,
    salary_to   INTEGER NOT NULL,
    tax_dep_0   INTEGER NOT NULL DEFAULT 0,
    tax_dep_1   INTEGER NOT NULL DEFAULT 0,
    tax_dep_2   INTEGER NOT NULL DEFAULT 0,
    tax_dep_3   INTEGER NOT NULL DEFAULT 0,
    tax_dep_4   INTEGER NOT NULL DEFAULT 0,
    tax_dep_5   INTEGER NOT NULL DEFAULT 0,
    tax_dep_6   INTEGER NOT NULL DEFAULT 0,
    tax_dep_7   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_wht_year_from ON m_withholding_tax(fiscal_year, salary_from);

-- 住民税スケジュール
CREATE TABLE IF NOT EXISTS m_resident_tax_schedule (
    id              BIGSERIAL PRIMARY KEY,
    employee_id     BIGINT NOT NULL REFERENCES m_employee(id) ON DELETE CASCADE,
    fiscal_year     INTEGER NOT NULL,
    municipality    VARCHAR(64) NOT NULL DEFAULT '',
    month_06        INTEGER NOT NULL DEFAULT 0,
    month_07        INTEGER NOT NULL DEFAULT 0,
    month_08        INTEGER NOT NULL DEFAULT 0,
    month_09        INTEGER NOT NULL DEFAULT 0,
    month_10        INTEGER NOT NULL DEFAULT 0,
    month_11        INTEGER NOT NULL DEFAULT 0,
    month_12        INTEGER NOT NULL DEFAULT 0,
    month_01        INTEGER NOT NULL DEFAULT 0,
    month_02        INTEGER NOT NULL DEFAULT 0,
    month_03        INTEGER NOT NULL DEFAULT 0,
    month_04        INTEGER NOT NULL DEFAULT 0,
    month_05        INTEGER NOT NULL DEFAULT 0,
    UNIQUE (employee_id, fiscal_year)
);

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- ワークフローマスタ (m_)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

CREATE TABLE IF NOT EXISTS m_workflow_definition (
    id          BIGSERIAL PRIMARY KEY,
    code        VARCHAR(30) NOT NULL UNIQUE,
    name        VARCHAR(100) NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS m_workflow_step (
    id                  BIGSERIAL PRIMARY KEY,
    definition_id       BIGINT NOT NULL REFERENCES m_workflow_definition(id) ON DELETE CASCADE,
    code                VARCHAR(30) NOT NULL,
    name                VARCHAR(20) NOT NULL,
    sort_order          INTEGER NOT NULL,
    is_terminal         BOOLEAN NOT NULL DEFAULT FALSE,
    icon                VARCHAR(10) NOT NULL DEFAULT '',
    allowed_mail_types  JSONB NOT NULL DEFAULT '[]',
    UNIQUE (definition_id, sort_order),
    UNIQUE (definition_id, code)
);

CREATE TABLE IF NOT EXISTS m_workflow_transition (
    id              BIGSERIAL PRIMARY KEY,
    from_step_id    BIGINT NOT NULL REFERENCES m_workflow_step(id) ON DELETE CASCADE,
    to_step_id      BIGINT NOT NULL REFERENCES m_workflow_step(id) ON DELETE CASCADE,
    is_back         BOOLEAN NOT NULL DEFAULT FALSE,
    description     TEXT NOT NULL DEFAULT '',
    UNIQUE (from_step_id, to_step_id)
);

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- トランザクション (t_)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- 発注注文書
CREATE TABLE IF NOT EXISTS t_purchase_order (
    order_id            VARCHAR(20) PRIMARY KEY,
    uuid                UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    status              VARCHAR(20) NOT NULL DEFAULT 'DRAFT',
    partner_id          VARCHAR(32) NOT NULL REFERENCES m_partner(partner_id) ON DELETE CASCADE,
    project_id          VARCHAR(32) NOT NULL REFERENCES m_project(project_id) ON DELETE RESTRICT,
    engineer_id         BIGINT REFERENCES m_engineer(id) ON DELETE RESTRICT,
    partner_contract_id BIGINT REFERENCES m_partner_contract(id) ON DELETE SET NULL,
    order_date          DATE NOT NULL DEFAULT CURRENT_DATE,
    work_start          DATE NOT NULL,
    work_end            DATE NOT NULL,
    workplace_id        BIGINT REFERENCES m_workplace(id) ON DELETE SET NULL,
    deliverable_text    VARCHAR(255) NOT NULL DEFAULT '月別作業報告書',
    payment_condition   TEXT NOT NULL DEFAULT '毎月末日締め翌月末日払い（税別）',
    contract_items      TEXT NOT NULL DEFAULT '',
    work_location       VARCHAR(255) NOT NULL DEFAULT '',
    甲_責任者           VARCHAR(64) NOT NULL DEFAULT '',
    甲_担当者           VARCHAR(64) NOT NULL DEFAULT '',
    乙_責任者           VARCHAR(64) NOT NULL DEFAULT '',
    乙_担当者           VARCHAR(64) NOT NULL DEFAULT '',
    作業責任者           VARCHAR(64) NOT NULL DEFAULT '',
    remarks             TEXT NOT NULL DEFAULT '',
    finalized_at        TIMESTAMPTZ,
    token_issued_at     TIMESTAMPTZ,
    document_hash       VARCHAR(64) NOT NULL DEFAULT '',
    order_pdf           VARCHAR(512) NOT NULL DEFAULT '',
    acceptance_pdf      VARCHAR(512) NOT NULL DEFAULT '',
    drive_file_id       VARCHAR(200) NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 発注明細
CREATE TABLE IF NOT EXISTS t_purchase_order_item (
    id                  BIGSERIAL PRIMARY KEY,
    order_id            VARCHAR(20) NOT NULL REFERENCES t_purchase_order(order_id) ON DELETE CASCADE,
    partner_contract_id BIGINT NOT NULL REFERENCES m_partner_contract(id) ON DELETE RESTRICT,
    base_fee            INTEGER NOT NULL DEFAULT 0,
    effort              DECIMAL(3,2) NOT NULL DEFAULT 1.00,
    actual_hours        DECIMAL(6,2) NOT NULL DEFAULT 0.00,
    settlement_type     VARCHAR(10) NOT NULL DEFAULT 'RANGE',
    lower_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 140.0,
    upper_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    fixed_hours         DECIMAL(5,1),
    deduction_rate      INTEGER NOT NULL DEFAULT 0,
    overtime_rate       INTEGER NOT NULL DEFAULT 0,
    price               INTEGER NOT NULL DEFAULT 0
);

-- 支払通知書
CREATE TABLE IF NOT EXISTS t_payment_notice (
    notice_id           VARCHAR(20) PRIMARY KEY,
    uuid                UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    purchase_order_id   VARCHAR(20) NOT NULL REFERENCES t_purchase_order(order_id) ON DELETE CASCADE,
    partner_id          VARCHAR(32) NOT NULL REFERENCES m_partner(partner_id) ON DELETE CASCADE,
    target_month        DATE NOT NULL,
    notice_date         DATE NOT NULL DEFAULT CURRENT_DATE,
    payment_due_date    DATE,
    subtotal            INTEGER NOT NULL DEFAULT 0,
    tax_amount          INTEGER NOT NULL DEFAULT 0,
    total               INTEGER NOT NULL DEFAULT 0,
    confirmed_at        TIMESTAMPTZ,
    confirmed_by_id     BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    notice_pdf          VARCHAR(512) NOT NULL DEFAULT '',
    remarks             TEXT NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 支払通知明細
CREATE TABLE IF NOT EXISTS t_payment_notice_item (
    id                  BIGSERIAL PRIMARY KEY,
    notice_id           VARCHAR(20) NOT NULL REFERENCES t_payment_notice(notice_id) ON DELETE CASCADE,
    partner_contract_id BIGINT NOT NULL REFERENCES m_partner_contract(id) ON DELETE RESTRICT,
    actual_hours        DECIMAL(6,2) NOT NULL DEFAULT 0.00,
    base_fee            INTEGER NOT NULL DEFAULT 0,
    effort              DECIMAL(3,2) NOT NULL DEFAULT 1.00,
    lower_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 140.0,
    upper_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    fixed_hours         DECIMAL(5,1),
    deduction_rate      INTEGER NOT NULL DEFAULT 0,
    overtime_rate       INTEGER NOT NULL DEFAULT 0,
    adjustment          INTEGER NOT NULL DEFAULT 0,
    amount              INTEGER NOT NULL DEFAULT 0
);

-- 受注注文書
CREATE TABLE IF NOT EXISTS t_received_order (
    id                      BIGSERIAL PRIMARY KEY,
    uuid                    UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    received_order_no       VARCHAR(20) NOT NULL UNIQUE,
    client_id               BIGINT NOT NULL REFERENCES m_client(id) ON DELETE RESTRICT,
    engineer_id             BIGINT REFERENCES m_engineer(id) ON DELETE RESTRICT,
    client_contract_id      BIGINT REFERENCES m_client_contract(id) ON DELETE SET NULL,
    client_order_number     VARCHAR(50) NOT NULL DEFAULT '',
    target_month            DATE NOT NULL,
    work_start              DATE NOT NULL,
    work_end                DATE NOT NULL,
    project_name            VARCHAR(255) NOT NULL DEFAULT '',
    payment_condition       TEXT NOT NULL DEFAULT '毎月末日締め翌月末日払い（税別）',
    status                  VARCHAR(20) NOT NULL DEFAULT 'REGISTERED',
    order_file              VARCHAR(512) NOT NULL DEFAULT '',
    parsed_data             JSONB,
    is_recurring            BOOLEAN NOT NULL DEFAULT FALSE,
    parent_order_id         BIGINT REFERENCES t_received_order(id) ON DELETE SET NULL,
    order_date              DATE NOT NULL DEFAULT CURRENT_DATE,
    remarks                 TEXT NOT NULL DEFAULT '',
    report_to_email         VARCHAR(255) NOT NULL DEFAULT '',
    report_cc_emails        TEXT NOT NULL DEFAULT '',
    invoice_to_email        VARCHAR(255) NOT NULL DEFAULT '',
    invoice_cc_emails       TEXT NOT NULL DEFAULT '',
    invoice_confirmed       BOOLEAN NOT NULL DEFAULT FALSE,
    invoice_confirmed_at    TIMESTAMPTZ,
    payment_confirmed       BOOLEAN NOT NULL DEFAULT FALSE,
    payment_confirmed_at    TIMESTAMPTZ,
    report_sent_to_client   BOOLEAN NOT NULL DEFAULT FALSE,
    report_sent_at          TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 受注明細（精算条件フィールド含む）
CREATE TABLE IF NOT EXISTS t_received_order_item (
    id                  BIGSERIAL PRIMARY KEY,
    order_id            BIGINT NOT NULL REFERENCES t_received_order(id) ON DELETE CASCADE,
    client_contract_id  BIGINT REFERENCES m_client_contract(id) ON DELETE RESTRICT,
    engineer_name       VARCHAR(128) NOT NULL DEFAULT '',
    unit_price          INTEGER NOT NULL DEFAULT 0,
    man_month           DECIMAL(4,2) NOT NULL DEFAULT 1.00,
    actual_hours        DECIMAL(6,2) NOT NULL DEFAULT 0,
    adjustment          INTEGER NOT NULL DEFAULT 0,
    amount              INTEGER NOT NULL DEFAULT 0,
    -- 精算条件スナップショット
    settlement_type     VARCHAR(10) NOT NULL DEFAULT 'RANGE',
    base_rate           INTEGER NOT NULL DEFAULT 0,
    lower_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 140.0,
    upper_limit_hours   DECIMAL(5,1) NOT NULL DEFAULT 180.0,
    fixed_hours         DECIMAL(5,1),
    deduction_rate      INTEGER NOT NULL DEFAULT 0,
    overtime_rate       INTEGER NOT NULL DEFAULT 0,
    effort              DECIMAL(3,2) NOT NULL DEFAULT 1.00,
    mid_month_rule      VARCHAR(10) NOT NULL DEFAULT 'FULL'
);

-- 請求書
CREATE TABLE IF NOT EXISTS t_billing_invoice (
    invoice_id  VARCHAR(20) NOT NULL UNIQUE,
    id          BIGSERIAL PRIMARY KEY,
    client_id   BIGINT NOT NULL REFERENCES m_client(id) ON DELETE RESTRICT,
    received_order_id BIGINT REFERENCES t_received_order(id) ON DELETE SET NULL,
    issue_date  DATE NOT NULL DEFAULT CURRENT_DATE,
    due_date    DATE,
    subject     VARCHAR(255) NOT NULL DEFAULT '',
    notes       TEXT NOT NULL DEFAULT '',
    pdf_file    VARCHAR(512) NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 請求明細
CREATE TABLE IF NOT EXISTS t_billing_item (
    id                      BIGSERIAL PRIMARY KEY,
    invoice_id              BIGINT NOT NULL REFERENCES t_billing_invoice(id) ON DELETE CASCADE,
    received_order_item_id  BIGINT REFERENCES t_received_order_item(id) ON DELETE SET NULL,
    product_name            VARCHAR(255) NOT NULL DEFAULT '',
    unit_price              INTEGER NOT NULL DEFAULT 0,
    man_month               DECIMAL(4,2) NOT NULL DEFAULT 1.00,
    actual_hours            DECIMAL(6,2) NOT NULL DEFAULT 0,
    adjustment              INTEGER NOT NULL DEFAULT 0,
    amount                  INTEGER NOT NULL DEFAULT 0,
    tax_category            VARCHAR(2) NOT NULL DEFAULT '10',
    sort_order              INTEGER NOT NULL DEFAULT 0
);

-- 入金記録
CREATE TABLE IF NOT EXISTS t_payment_record (
    id              BIGSERIAL PRIMARY KEY,
    invoice_id      BIGINT NOT NULL REFERENCES t_billing_invoice(id) ON DELETE CASCADE,
    payment_date    DATE NOT NULL,
    amount          INTEGER NOT NULL,
    method          VARCHAR(10) NOT NULL DEFAULT 'TRANSFER',
    reference       VARCHAR(255) NOT NULL DEFAULT '',
    confirmed_by_id BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    confirmed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 月次稼働報告
CREATE TABLE IF NOT EXISTS t_monthly_timesheet (
    id                  BIGSERIAL PRIMARY KEY,
    client_contract_id  BIGINT NOT NULL REFERENCES m_client_contract(id) ON DELETE RESTRICT,
    target_month        DATE NOT NULL,
    status              VARCHAR(10) NOT NULL DEFAULT 'UPLOADED',
    total_hours         DECIMAL(6,2) NOT NULL DEFAULT 0,
    work_days           INTEGER NOT NULL DEFAULT 0,
    overtime_hours      DECIMAL(6,2) NOT NULL DEFAULT 0,
    night_hours         DECIMAL(6,2) NOT NULL DEFAULT 0,
    holiday_hours       DECIMAL(6,2) NOT NULL DEFAULT 0,
    daily_data          JSONB,
    excel_file          VARCHAR(512) NOT NULL DEFAULT '',
    pdf_file            VARCHAR(512) NOT NULL DEFAULT '',
    original_filename   VARCHAR(512) NOT NULL DEFAULT '',
    drive_file_id       VARCHAR(200) NOT NULL DEFAULT '',
    uploaded_by_id      BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    uploaded_at         TIMESTAMPTZ,
    alerts_json         JSONB,
    error_message       TEXT NOT NULL DEFAULT '',
    sent_to_client_at   TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (client_contract_id, target_month)
);

-- 受信メール
CREATE TABLE IF NOT EXISTS t_received_email (
    id                  BIGSERIAL PRIMARY KEY,
    message_id          VARCHAR(512) NOT NULL UNIQUE,
    from_email          VARCHAR(255) NOT NULL,
    from_name           VARCHAR(255) NOT NULL DEFAULT '',
    subject             VARCHAR(512) NOT NULL,
    received_at         TIMESTAMPTZ NOT NULL,
    body_text           TEXT NOT NULL DEFAULT '',
    partner_id          VARCHAR(32) REFERENCES m_partner(partner_id) ON DELETE SET NULL,
    status              VARCHAR(20) NOT NULL DEFAULT 'NEW',
    timesheet_id        BIGINT REFERENCES t_monthly_timesheet(id) ON DELETE SET NULL,
    error_message       TEXT NOT NULL DEFAULT '',
    attachment_filename VARCHAR(512) NOT NULL DEFAULT '',
    attachment_file     VARCHAR(512) NOT NULL DEFAULT '',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    processed_at        TIMESTAMPTZ
);

-- 月次タスク
CREATE TABLE IF NOT EXISTS t_monthly_task (
    id                  BIGSERIAL PRIMARY KEY,
    project_id          VARCHAR(32) NOT NULL REFERENCES m_project(project_id) ON DELETE CASCADE,
    engineer_id         BIGINT REFERENCES m_engineer(id) ON DELETE CASCADE,
    work_month          DATE NOT NULL,
    task_type           VARCHAR(20) NOT NULL,
    responsible         VARCHAR(10) NOT NULL DEFAULT 'STAFF',
    deadline            DATE NOT NULL,
    status              VARCHAR(15) NOT NULL DEFAULT 'PENDING',
    completed_at        TIMESTAMPTZ,
    note                TEXT NOT NULL DEFAULT '',
    reminder_sent       BOOLEAN NOT NULL DEFAULT FALSE,
    alert_sent          BOOLEAN NOT NULL DEFAULT FALSE,
    deadline_notified   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, engineer_id, work_month, task_type)
);

-- 給与
CREATE TABLE IF NOT EXISTS t_payroll (
    id                      BIGSERIAL PRIMARY KEY,
    employee_id             BIGINT NOT NULL REFERENCES m_employee(id) ON DELETE RESTRICT,
    year_month              DATE NOT NULL,
    timesheet_id            BIGINT REFERENCES t_monthly_timesheet(id) ON DELETE SET NULL,
    status                  VARCHAR(10) NOT NULL DEFAULT 'DRAFT',
    payment_date            DATE,
    work_days               INTEGER NOT NULL DEFAULT 0,
    total_hours             DECIMAL(6,2) NOT NULL DEFAULT 0,
    overtime_hours          DECIMAL(6,2) NOT NULL DEFAULT 0,
    night_hours             DECIMAL(6,2) NOT NULL DEFAULT 0,
    holiday_hours           DECIMAL(6,2) NOT NULL DEFAULT 0,
    absence_days            INTEGER NOT NULL DEFAULT 0,
    base_salary             INTEGER NOT NULL DEFAULT 0,
    position_allowance      INTEGER NOT NULL DEFAULT 0,
    housing_allowance       INTEGER NOT NULL DEFAULT 0,
    commuting_allowance     INTEGER NOT NULL DEFAULT 0,
    overtime_pay            INTEGER NOT NULL DEFAULT 0,
    night_pay               INTEGER NOT NULL DEFAULT 0,
    holiday_pay             INTEGER NOT NULL DEFAULT 0,
    absence_deduction       INTEGER NOT NULL DEFAULT 0,
    gross_pay               INTEGER NOT NULL DEFAULT 0,
    pension_premium         INTEGER NOT NULL DEFAULT 0,
    health_premium          INTEGER NOT NULL DEFAULT 0,
    nursing_premium         INTEGER NOT NULL DEFAULT 0,
    employment_premium      INTEGER NOT NULL DEFAULT 0,
    social_insurance_total  INTEGER NOT NULL DEFAULT 0,
    income_tax              INTEGER NOT NULL DEFAULT 0,
    resident_tax            INTEGER NOT NULL DEFAULT 0,
    deduction_total         INTEGER NOT NULL DEFAULT 0,
    net_pay                 INTEGER NOT NULL DEFAULT 0,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (employee_id, year_month)
);

-- 経費申請（PayrollSystem から移行）
CREATE TABLE IF NOT EXISTS t_expense_request (
    id              BIGSERIAL PRIMARY KEY,
    employee_id     BIGINT NOT NULL REFERENCES m_employee(id) ON DELETE RESTRICT,
    expense_date    DATE NOT NULL,
    category        VARCHAR(30) NOT NULL DEFAULT 'OTHER',
    amount          INTEGER NOT NULL DEFAULT 0,
    description     TEXT NOT NULL DEFAULT '',
    receipt_file    VARCHAR(512) NOT NULL DEFAULT '',
    status          VARCHAR(10) NOT NULL DEFAULT 'DRAFT',
    approved_by_id  BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    approved_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ワークフローインスタンス
CREATE TABLE IF NOT EXISTS t_workflow_instance (
    id                  BIGSERIAL PRIMARY KEY,
    definition_id       BIGINT NOT NULL REFERENCES m_workflow_definition(id) ON DELETE RESTRICT,
    target_type         VARCHAR(30) NOT NULL,
    target_id           VARCHAR(32) NOT NULL,
    current_step_id     BIGINT NOT NULL REFERENCES m_workflow_step(id) ON DELETE RESTRICT,
    started_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at        TIMESTAMPTZ,
    is_completed        BOOLEAN NOT NULL DEFAULT FALSE,
    UNIQUE (target_type, target_id, definition_id)
);

-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
-- 履歴 (h_)
-- ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

-- メール送信ログ
CREATE TABLE IF NOT EXISTS h_sent_email (
    id          BIGSERIAL PRIMARY KEY,
    partner_id  VARCHAR(32) REFERENCES m_partner(partner_id) ON DELETE CASCADE,
    subject     VARCHAR(255) NOT NULL,
    body        TEXT NOT NULL,
    recipient   VARCHAR(255) NOT NULL DEFAULT '',
    sent_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ワークフロー遷移ログ
CREATE TABLE IF NOT EXISTS h_workflow_log (
    id              BIGSERIAL PRIMARY KEY,
    target_type     VARCHAR(30) NOT NULL,
    target_id       VARCHAR(32) NOT NULL,
    from_step_id    BIGINT REFERENCES m_workflow_step(id) ON DELETE SET NULL,
    to_step_id      BIGINT REFERENCES m_workflow_step(id) ON DELETE SET NULL,
    changed_by_id   BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    note            TEXT NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- メールスキャンログ
CREATE TABLE IF NOT EXISTS t_mail_scan_log (
    id                      BIGSERIAL PRIMARY KEY,
    gmail_message_id        VARCHAR(255) NOT NULL DEFAULT '',
    sender_email            VARCHAR(255) NOT NULL DEFAULT '',
    sender_name             VARCHAR(255) NOT NULL DEFAULT '',
    subject                 TEXT NOT NULL DEFAULT '',
    body_text               TEXT NOT NULL DEFAULT '',
    attachments             JSONB DEFAULT '[]',
    received_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    classification          VARCHAR(50) NOT NULL DEFAULT '',
    matched_entity_name     VARCHAR(255) NOT NULL DEFAULT '',
    is_reflected            BOOLEAN NOT NULL DEFAULT FALSE,
    note                    TEXT NOT NULL DEFAULT '',
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (gmail_message_id)
);

-- 作業場所マスタ
CREATE TABLE IF NOT EXISTS m_work_location (
    id          BIGSERIAL PRIMARY KEY,
    name        VARCHAR(255) NOT NULL DEFAULT '',
    address     VARCHAR(255) NOT NULL DEFAULT '',
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
