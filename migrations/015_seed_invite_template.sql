-- ============================================================
-- 015_seed_invite_template.sql — エンジニア招待メールテンプレート
-- ============================================================

INSERT INTO s_email_template (code, subject, body, description)
VALUES (
    'INVITE_ENGINEER',
    '【Sophia】パートナーポータルへの招待',
    '{{ engineer_name }} 様

Sophia パートナーポータルへの招待です。
以下のリンクをクリックするとログインできます。パスワードの設定は不要です。

{{ invite_url }}

※このリンクの有効期限は72時間です。
※ログイン後は30日間有効です。

Sophia 管理チーム',
    'エンジニア招待メールテンプレート（マジックリンク）'
) ON CONFLICT (code) DO UPDATE SET
    body = EXCLUDED.body,
    description = EXCLUDED.description;

