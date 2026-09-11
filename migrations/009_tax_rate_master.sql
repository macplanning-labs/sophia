-- 009: 消費税率マスタ — s_company_info から分離

CREATE TABLE IF NOT EXISTS m_tax_rate (
    id              BIGSERIAL PRIMARY KEY,
    rate            NUMERIC(5,2) NOT NULL,
    effective_from  DATE NOT NULL,
    description     VARCHAR(64) NOT NULL DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tax_rate_effective ON m_tax_rate(effective_from);

INSERT INTO m_tax_rate (rate, effective_from, description) VALUES
(3.00,  '1989-04-01', '消費税 3%'),
(5.00,  '1997-04-01', '消費税 5%'),
(8.00,  '2014-04-01', '消費税 8%'),
(10.00, '2019-10-01', '消費税 10%')
ON CONFLICT (effective_from) DO NOTHING;

ALTER TABLE s_company_info DROP COLUMN IF EXISTS tax_rate;
