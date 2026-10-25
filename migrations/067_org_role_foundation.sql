-- 067_org_role_foundation.sql
-- 目的: 組織・ロール・割り当ての表を作る(ロールと組織に基づく閲覧範囲の土台)。
--       初期データ(標準ロール等)は 068 で入れる。この表だけでは、既存の画面の挙動は変わらない。
-- チケット: DEMO-000173
-- 注意: 適用済みのマイグレーションは、コメント1行でも変更しない。

-- 組織(木構造。ルートは1つだけ)
CREATE TABLE IF NOT EXISTS m_org_unit (
    id          BIGSERIAL PRIMARY KEY,
    parent_id   BIGINT REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    name        VARCHAR(100) NOT NULL,
    unit_kind   VARCHAR(50),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_org_unit_not_self_parent CHECK (parent_id IS NULL OR parent_id <> id)
);
-- ルートは1つだけ
CREATE UNIQUE INDEX IF NOT EXISTS uq_org_unit_single_root ON m_org_unit ((1)) WHERE parent_id IS NULL;
-- 同じ親の下で、有効な組織の名前は重複しない
CREATE UNIQUE INDEX IF NOT EXISTS uq_org_unit_sibling_name ON m_org_unit (COALESCE(parent_id, 0), name) WHERE is_active;

-- ロール(権限のまとまり)
CREATE TABLE IF NOT EXISTS m_role (
    id          BIGSERIAL PRIMARY KEY,
    code        VARCHAR(50) NOT NULL UNIQUE,
    name        VARCHAR(100) NOT NULL,
    is_system   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ロールが持つ権限と、その見える範囲(スコープ)
CREATE TABLE IF NOT EXISTS m_role_permission (
    role_id          BIGINT NOT NULL REFERENCES m_role(id) ON DELETE CASCADE,
    permission_code  VARCHAR(100) NOT NULL,
    scope            VARCHAR(20) NOT NULL,
    PRIMARY KEY (role_id, permission_code),
    CONSTRAINT chk_role_permission_scope CHECK (scope IN ('PRIVATE','UNIT','HIERARCHICAL','GLOBAL'))
);

-- ユーザーへの割り当て(どの組織で、どのロールか)
CREATE TABLE IF NOT EXISTS s_user_org_role (
    id           BIGSERIAL PRIMARY KEY,
    user_id      BIGINT NOT NULL REFERENCES s_user(id) ON DELETE CASCADE,
    org_unit_id  BIGINT NOT NULL REFERENCES m_org_unit(id) ON DELETE RESTRICT,
    role_id      BIGINT NOT NULL REFERENCES m_role(id) ON DELETE RESTRICT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by   BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    UNIQUE (user_id, org_unit_id, role_id)
);
CREATE INDEX IF NOT EXISTS idx_user_org_role_user ON s_user_org_role(user_id);

-- 組織・ロール・割り当ての変更履歴
CREATE TABLE IF NOT EXISTS h_org_role_change (
    id           BIGSERIAL PRIMARY KEY,
    changed_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    changed_by   BIGINT REFERENCES s_user(id) ON DELETE SET NULL,
    target_kind  VARCHAR(30) NOT NULL,     -- org_unit / role / role_permission / assignment / owner_transfer
    target_id    BIGINT,
    action       VARCHAR(20) NOT NULL,     -- create / update / delete / move / transfer
    before_json  JSONB,
    after_json   JSONB
);

COMMENT ON TABLE m_org_unit IS '組織(木構造。ルートは1つだけ)';
COMMENT ON COLUMN m_org_unit.parent_id IS '親の組織ID(ルートはNULL)';
COMMENT ON COLUMN m_org_unit.unit_kind IS '組織の種別(部・課・チームなど。表示用)';
COMMENT ON COLUMN m_org_unit.is_active IS '有効フラグ(無効な組織への割り当ては権限として働かない。木には残る)';

COMMENT ON TABLE m_role IS 'ロール(権限のまとまり)';
COMMENT ON COLUMN m_role.code IS 'ロールの識別コード(例: admin, sales)';
COMMENT ON COLUMN m_role.is_system IS '標準・移行用ロール(TRUEは削除不可。権限セットの変更は可)';

COMMENT ON TABLE m_role_permission IS 'ロールが持つ権限と見える範囲';
COMMENT ON COLUMN m_role_permission.permission_code IS '権限コード(例: contract:view)';
COMMENT ON COLUMN m_role_permission.scope IS '見える範囲(PRIVATE=本人 / UNIT=その組織 / HIERARCHICAL=配下を含む / GLOBAL=全社)';

COMMENT ON TABLE s_user_org_role IS 'ユーザーの割り当て(組織 × ロール)';
COMMENT ON COLUMN s_user_org_role.org_unit_id IS '割り当ての組織(権限の判定の起点)';
COMMENT ON COLUMN s_user_org_role.created_by IS '割り当てを行ったユーザー';

COMMENT ON TABLE h_org_role_change IS '組織・ロール・割り当ての変更履歴';
COMMENT ON COLUMN h_org_role_change.target_kind IS '変更対象の種類(org_unit / role / role_permission / assignment / owner_transfer)';
COMMENT ON COLUMN h_org_role_change.action IS '操作(create / update / delete / move / transfer)';
