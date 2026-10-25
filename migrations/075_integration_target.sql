-- 075_integration_target.sql — 外部連携のつなぎ先を画面で設定する(DEMO-000215)
-- 設計: docs/design/sophia_000210_integration_design.md 3章(D8)
--
-- - 1つの用途(integrations.toml の id)に、送り先を複数つなげられる(1:N)。保存先(file_store)はアプリ側で1つに制限する。
-- - 秘密の設定(Webhook の URL・サービスアカウントの鍵)は、ログイン用の鍵とは別の専用の鍵で暗号化して保存する
--   (AES-256-GCM。どの鍵で暗号化したかを secret_key_id に残し、鍵を入れ替えられるようにする)。
-- - 画面・API には秘密の値を出さない。変更の記録(h_integration_change)にも残さない。
-- - 表を追加するだけ。既存の表は変えない。行が無い用途は、移行の間、サーバの設定ファイル(環境変数)を読む。

CREATE TABLE IF NOT EXISTS s_integration_target (
    id                   BIGSERIAL PRIMARY KEY,
    use_id               TEXT        NOT NULL,
    label                TEXT        NOT NULL,
    provider             TEXT        NOT NULL,           -- 'gchat' | 'gdrive' …
    config               JSONB       NOT NULL DEFAULT '{}'::jsonb,   -- 秘密でない設定(フォルダ ID など)
    secret_enc           BYTEA,                           -- 秘密の設定(JSON)を暗号化したもの
    secret_nonce         BYTEA,
    secret_key_id        TEXT,
    enabled              BOOLEAN     NOT NULL DEFAULT TRUE,
    last_check_at        TIMESTAMPTZ,
    last_check_ok        BOOLEAN,
    last_check_detail    JSONB,                           -- 項目ごとの結果(秘密の値は含めない)
    last_test_at         TIMESTAMPTZ,
    last_test_ok         BOOLEAN,
    last_test_by         BIGINT,
    last_test_detail     TEXT,
    last_delivery_at     TIMESTAMPTZ,
    last_delivery_ok     BOOLEAN,
    last_delivery_detail TEXT,
    updated_by           BIGINT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT uq_integration_target_label UNIQUE (use_id, label),
    CONSTRAINT ck_integration_target_secret CHECK (
        (secret_enc IS NULL AND secret_nonce IS NULL AND secret_key_id IS NULL)
        OR (secret_enc IS NOT NULL AND secret_nonce IS NOT NULL AND secret_key_id IS NOT NULL)
    )
);

COMMENT ON TABLE s_integration_target IS '外部連携のつなぎ先(用途ごとに複数)。秘密の設定は専用の鍵で暗号化して保存する';

CREATE TABLE IF NOT EXISTS h_integration_change (
    id          BIGSERIAL PRIMARY KEY,
    changed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    changed_by  BIGINT,
    use_id      TEXT        NOT NULL,
    target_id   BIGINT,
    summary     TEXT        NOT NULL      -- 何を変えたか(秘密の値は書かない)
);

CREATE INDEX IF NOT EXISTS idx_integration_change_at ON h_integration_change (changed_at DESC);

COMMENT ON TABLE h_integration_change IS '外部連携の設定の変更の記録(秘密の値は残さない)';
