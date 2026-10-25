/// infrastructure/repositories/attempt_lock_store.rs — auth_core::domain::attempt_lock::AttemptStore の DB 実装
///
/// 以前は `InMemoryAttemptStore`(プロセス内のメモリ)で数えていたため、再起動で全部消え、複数台でも共有されなかった。
/// 今は `s_login_attempt`(`login_attempt_repo`)に記録する(DEMO-000233 S2-D / 項目6-1)。
/// 使うのは、ポータルのマジックリンク(`kind = portal`)と、二要素認証の確認(`kind = mfa`)。
/// パスワードログインは、アカウント単位と IP 単位の 2 つを数えるため `login_attempt_repo::reserve_password_attempt` を直接使う。
///
/// キーは「IP:識別子」の文字列をそのまま SHA-256 にして保存する(メールアドレスを平文で残さない)。
/// 数える窓(`window`)と上限(`max_attempts`)は、生成時の `AttemptLockConfig` を使う
/// (トレイトの `cfg` 引数は、呼び出し側が同じ設定を渡している前提で、ここでは使わない)。
use auth_core::domain::attempt_lock::{AttemptLockConfig, AttemptStore};
use auth_core::error::AuthError;
use sqlx::PgPool;

use crate::infrastructure::repositories::login_attempt_repo as repo;

pub struct PgAttemptStore {
    pool: PgPool,
    kind: &'static str,
    cfg: AttemptLockConfig,
}

impl PgAttemptStore {
    pub fn new(pool: PgPool, kind: &'static str, cfg: AttemptLockConfig) -> Self {
        Self { pool, kind, cfg }
    }

    fn window_secs(&self) -> i64 {
        self.cfg.window.as_secs() as i64
    }

    fn max(&self) -> i64 {
        i64::from(self.cfg.max_attempts)
    }

    /// 試行を 1 件記録してから数える(判定の前に記録する。DEMO-000233 S2-D / 項目6-1 の修正)。
    /// 確認→記録→再確認の 3 手だと、同時に送られた試行が全部「まだ記録されていない」ものとして通ってしまう。
    /// 記録してから数えれば、先に記録された行は後の数えから必ず見えるので、上限を超えて通らない
    /// (読み取り時点の行は、INSERT が文ごとにコミット済みのため)。
    ///
    /// - `Ok(None)`: 試行してよい(記録した 1 件は残す。失敗なら数えとして残り、成功なら `reset` で消す)。
    /// - `Ok(Some(秒))`: ロック中。記録した 1 件は消す(ロック中の試行は記録しない)。`秒` のあと再試行できる。
    pub async fn reserve(&self, key: &str) -> Result<Option<u64>, AuthError> {
        let key_hash = repo::hash_key(key);
        let id = repo::insert_attempt(&self.pool, self.kind, &key_hash, None)
            .await
            .map_err(db_error)?;
        let count = repo::count_failures(&self.pool, self.kind, &key_hash, None, self.window_secs())
            .await
            .map_err(db_error)?;
        if count <= self.max() {
            return Ok(None);
        }
        repo::delete_attempt(&self.pool, id).await.map_err(db_error)?;
        let secs = repo::lock_remaining_secs(&self.pool, self.kind, &key_hash, None, self.window_secs(), self.max())
            .await
            .map_err(db_error)?;
        Ok(Some(secs.unwrap_or(1).max(1) as u64))
    }
}

fn db_error(e: sqlx::Error) -> AuthError {
    tracing::error!("試行回数の保存に失敗: {:?}", e);
    AuthError::Internal("試行回数の保存に失敗しました".to_string())
}

#[async_trait::async_trait]
impl AttemptStore for PgAttemptStore {
    async fn record_failure(&self, key: &str, _cfg: &AttemptLockConfig) -> Result<(), AuthError> {
        repo::insert_attempt(&self.pool, self.kind, &repo::hash_key(key), None)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    async fn reset(&self, key: &str) -> Result<(), AuthError> {
        repo::invalidate_failures(&self.pool, self.kind, &repo::hash_key(key), None)
            .await
            .map(|_| ())
            .map_err(db_error)
    }

    async fn locked_retry_after_seconds(&self, key: &str) -> Result<Option<u64>, AuthError> {
        let key_hash = repo::hash_key(key);
        let count = repo::count_failures(&self.pool, self.kind, &key_hash, None, self.window_secs())
            .await
            .map_err(db_error)?;
        if count < self.max() {
            return Ok(None);
        }
        let secs = repo::lock_remaining_secs(&self.pool, self.kind, &key_hash, None, self.window_secs(), self.max())
            .await
            .map_err(db_error)?;
        Ok(Some(secs.unwrap_or(1).max(1) as u64))
    }
}
