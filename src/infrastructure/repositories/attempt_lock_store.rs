/// infrastructure/repositories/attempt_lock_store.rs — auth_core::domain::attempt_lock::AttemptStore のインメモリ実装（社員/管理者ログイン試行制限）
///
/// このファイルは src/presentation/login_guard.rs の login_key 名前空間ロジックを新しいトレイト形状にポートしたものです。
/// login_guard.rs 自身は変更していません（まだ portal_key・mfa_key を提供しています）。
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use auth_core::domain::attempt_lock::{AttemptLockConfig, AttemptStore};
use auth_core::error::AuthError;

#[derive(Debug, Clone)]
struct Bucket {
    failures: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

/// インメモリ試行回数制限ストア
#[derive(Debug)]
pub struct InMemoryAttemptStore {
    store: Mutex<HashMap<String, Bucket>>,
}

impl InMemoryAttemptStore {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }

    fn now() -> Instant {
        Instant::now()
    }
}

impl Default for InMemoryAttemptStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AttemptStore for InMemoryAttemptStore {
    async fn record_failure(&self, key: &str, cfg: &AttemptLockConfig) -> Result<(), AuthError> {
        let mut map = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let t = Self::now();

        let bucket = map.entry(key.to_string()).or_insert_with(|| Bucket {
            failures: 0,
            window_start: t,
            locked_until: None,
        });

        // チェック: 現在ロック中か
        if let Some(until) = bucket.locked_until {
            if t < until {
                // まだロック中 — 何もしない
                return Ok(());
            }
            // ロック期限が切れた → リセット
            bucket.locked_until = None;
            bucket.failures = 0;
            bucket.window_start = t;
        }

        // ウィンドウ期限が切れたか
        if t.duration_since(bucket.window_start) > cfg.window {
            bucket.failures = 0;
            bucket.window_start = t;
        }

        // 失敗回数を増加
        bucket.failures = bucket.failures.saturating_add(1);

        // max_attempts に達したかチェック
        if bucket.failures >= cfg.max_attempts {
            bucket.locked_until = Some(t + cfg.lock_duration);
            bucket.failures = 0;
            bucket.window_start = t;
        }

        Ok(())
    }

    async fn reset(&self, key: &str) -> Result<(), AuthError> {
        let mut map = self.store.lock().unwrap_or_else(|e| e.into_inner());
        map.remove(key);
        Ok(())
    }

    async fn locked_retry_after_seconds(&self, key: &str) -> Result<Option<u64>, AuthError> {
        let mut map = self.store.lock().unwrap_or_else(|e| e.into_inner());
        let Some(bucket) = map.get_mut(key) else {
            return Ok(None);
        };

        let t = Self::now();

        // ロック中かチェック
        if let Some(until) = bucket.locked_until {
            if t < until {
                let remaining = until.saturating_duration_since(t).as_secs().max(1);
                return Ok(Some(remaining));
            }
            // ロック期限が切れた → 表面上はロックなし
            bucket.locked_until = None;
            bucket.failures = 0;
            bucket.window_start = t;
        }

        // ウィンドウ期限が切れたか
        // （これは locked_retry_after_seconds 呼び出し時に確認する必要がないが、
        // record_failure 側との一貫性のため確認してからカウンターをリセット）
        // ただし、この関数は呼び出し側で lock status を確認するだけなので、
        // ウィンドウ期限切れ時にカウンターをリセットする必要はない
        // （record_failure が次に呼ばれる時にリセットされる）
        // ここはシンプルに、ロックしていなければ None を返す

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn locks_after_five_failures() {
        let store = InMemoryAttemptStore::new();
        let cfg = AttemptLockConfig::default();
        let key = "test:login:user@example.com";

        // 4回失敗 — ロックなし
        for _ in 0..4 {
            store.record_failure(key, &cfg).await.unwrap();
            let status = store.locked_retry_after_seconds(key).await.unwrap();
            assert_eq!(status, None, "should not be locked yet");
        }

        // 5回目の失敗
        store.record_failure(key, &cfg).await.unwrap();
        let status = store.locked_retry_after_seconds(key).await.unwrap();
        assert!(status.is_some(), "should be locked after 5 failures");
        assert!(status.unwrap() > 0, "retry_after_secs should be > 0");
    }

    #[tokio::test]
    async fn success_resets_failures() {
        let store = InMemoryAttemptStore::new();
        let cfg = AttemptLockConfig::default();
        let key = "test:login:ok@example.com";

        // 3回失敗
        for _ in 0..3 {
            store.record_failure(key, &cfg).await.unwrap();
        }

        // リセット
        store.reset(key).await.unwrap();

        // ロック状態じゃない
        let status = store.locked_retry_after_seconds(key).await.unwrap();
        assert_eq!(status, None);

        // 新しいラウンドで4回失敗 — まだロックしない
        for _ in 0..4 {
            store.record_failure(key, &cfg).await.unwrap();
        }
        let status = store.locked_retry_after_seconds(key).await.unwrap();
        assert_eq!(
            status, None,
            "should not be locked after reset + 4 failures"
        );

        // 5回目でロック
        store.record_failure(key, &cfg).await.unwrap();
        let status = store.locked_retry_after_seconds(key).await.unwrap();
        assert!(status.is_some(), "should be locked after 5 failures");
    }

    #[tokio::test]
    async fn lock_expires_after_duration() {
        let store = InMemoryAttemptStore::new();
        let cfg = AttemptLockConfig {
            window: Duration::from_millis(50),
            max_attempts: 5,
            lock_duration: Duration::from_millis(50),
            max_body_bytes: 32 * 1024,
        };
        let key = "test:lock:expiry";

        // 5回失敗してロック
        for _ in 0..5 {
            store.record_failure(key, &cfg).await.unwrap();
        }

        // ロック中であることを確認
        let status = store.locked_retry_after_seconds(key).await.unwrap();
        assert!(
            status.is_some(),
            "should be locked immediately after reaching max_attempts"
        );

        // ロック期限が切れるまで待つ
        tokio::time::sleep(Duration::from_millis(80)).await;

        // ロックが解除されたことを確認
        let status = store.locked_retry_after_seconds(key).await.unwrap();
        assert_eq!(
            status, None,
            "should not be locked after lock_duration expires"
        );
    }
}
