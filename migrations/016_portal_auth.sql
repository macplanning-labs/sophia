-- ============================================================
-- 016_portal_auth.sql — 技術者マジックリンク認証
-- ============================================================

-- セッションテーブル: エンジニアセッション用カラム追加
-- user_id は NOT NULL 制約があるため、エンジニアセッションでは
-- ダミー値(0)は使えない。user_id を NULL 許可に変更する。
ALTER TABLE s_session
    ALTER COLUMN user_id DROP NOT NULL;

ALTER TABLE s_session
    ADD COLUMN IF NOT EXISTS engineer_id BIGINT REFERENCES m_engineer(id) ON DELETE CASCADE;

-- エンジニアログイントークン（マジックリンク用）
CREATE TABLE IF NOT EXISTS s_engineer_login_token (
    id           BIGSERIAL PRIMARY KEY,
    engineer_id  BIGINT NOT NULL REFERENCES m_engineer(id) ON DELETE CASCADE,
    token        UUID NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    expires_at   TIMESTAMPTZ NOT NULL,
    is_used      BOOLEAN NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_engineer_login_token
    ON s_engineer_login_token(token) WHERE is_used = false;

-- ログインリンクメールテンプレート
INSERT INTO s_email_template (code, subject, body, description)
VALUES (
    'ENGINEER_LOGIN_LINK',
    '【Sophia】ログインリンク',
    '{{ engineer_name }} 様

Sophia 稼働報告ポータルへのログインリンクです。
以下のURLをクリックしてログインしてください。

{{ login_url }}

※このリンクの有効期限は24時間です。
※ログイン後は30日間有効です。

Sophia 管理チーム',
    '技術者マジックリンクログイン用テンプレート'
) ON CONFLICT (code) DO NOTHING;
