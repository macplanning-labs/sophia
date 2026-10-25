-- 082_login_attempt.sql — ログインの試行(パスワード・ポータル・二要素認証の確認)を DB に保存する(DEMO-000233 S2-D / 項目6-1)
--
-- これまでは、プロセス内のメモリ(InMemoryAttemptStore)で数えていたため、再起動で全部消え、複数台でも共有されなかった。
-- ここに 1 試行 1 行で記録し、件数は DB で数える(再起動をまたいでも残る)。
--   kind:       password(社員・管理者のパスワードログイン) / portal(パートナーのポータル) / mfa(二要素認証の確認)
--   key_hash:   キーの SHA-256(16 進)。メールアドレスそのものは保存しない
--   ip:         接続元 IP(取れないときは NULL)。「IP + アカウント」の数え方と、成功の無効化(成功した IP の行だけ)に使う
--   succeeded:  成功で無効化された行。数えない(行は残す)
--   created_at: 数える時間窓の起点。7 日を過ぎた行は、定期の掃除で消す
CREATE TABLE IF NOT EXISTS s_login_attempt (
    id          BIGSERIAL PRIMARY KEY,
    kind        VARCHAR(16)  NOT NULL,
    key_hash    VARCHAR(64)  NOT NULL,
    ip          INET,
    succeeded   BOOLEAN      NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- 数える(種別 + キー + 時間窓)と、古い順の掃除の両方に使う。成功済みの行は数えないので、部分索引にする
CREATE INDEX IF NOT EXISTS idx_login_attempt_key_window
    ON s_login_attempt (kind, key_hash, created_at)
    WHERE succeeded = FALSE;

-- 期限切れの行を、古い順に消すための索引
CREATE INDEX IF NOT EXISTS idx_login_attempt_created
    ON s_login_attempt (created_at);
