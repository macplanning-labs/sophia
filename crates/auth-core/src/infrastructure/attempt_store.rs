//! infrastructure/attempt_store.rs — `AttemptStore` のメモリ内実装（プロセス内 Mutex<HashMap>）
//!
//! 別のシステムで使っているメモリ内のストアを元に、次の2点を足したもの(DEMO-000153)。
//! - **メモリの上限**: 攻撃者が毎回違う名前で失敗させても、表が際限なく大きくならないようにする。
//!   期限切れの行はときどき(一定回数の記録ごと・満杯のとき)まとめて消し、それでも満杯なら
//!   「ロック中でない行のうち、いちばん古いもの」を1件追い出す。ロック中の行は、できるだけ残す。
//! - **キーをそのまま持たない**: キー(IP・ユーザー名・MFA トークンなど)は SHA-256 にしてから持つ。
//!   1行の大きさが一定になり、メモリ上にユーザー名やトークンが平文で残らない。
//!
//! 注意: 数はプロセスごとに持つ。複数台で動かすと、台ごとに別々に数える(上限は台数倍になる)。
//! 再起動すると数は消える。複数台で共有したいときは、DB/Redis の実装を別に用意する。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::domain::attempt_lock::{AttemptLockConfig, AttemptStore};
use crate::error::AuthError;

/// 表に持つ行数の既定の上限。1行はおよそ100バイト前後なので、10万行で10MB程度。
pub const DEFAULT_MAX_ENTRIES: usize = 100_000;

/// 何回記録するごとに、期限切れの行をまとめて消すか。
const PURGE_EVERY: u64 = 1024;

type KeyDigest = [u8; 32];

#[derive(Debug, Clone, Copy)]
struct Bucket {
    failures: u32,
    window_start: Instant,
    window: Duration,
    locked_until: Option<Instant>,
}

impl Bucket {
    /// この行が、もう何の意味も持たなくなる時刻(ロック明け、または数える時間窓の終わり)。
    fn expires_at(&self) -> Instant {
        match self.locked_until {
            Some(until) => until,
            None => self.window_start + self.window,
        }
    }

    fn is_locked_at(&self, now: Instant) -> bool {
        matches!(self.locked_until, Some(until) if now < until)
    }
}

#[derive(Debug)]
struct Inner {
    map: HashMap<KeyDigest, Bucket>,
    records_since_purge: u64,
}

/// メモリ内の試行回数ストア。ルート(用途)ごとに別のインスタンスを作ること
/// (同じキーが別の用途の数と混ざらないように)。
#[derive(Debug)]
pub struct InMemoryAttemptStore {
    inner: Mutex<Inner>,
    max_entries: usize,
}

impl InMemoryAttemptStore {
    /// 既定の上限(`DEFAULT_MAX_ENTRIES`)で作る。
    pub fn new() -> Self {
        Self::with_max_entries(DEFAULT_MAX_ENTRIES)
    }

    /// 表に持つ行数の上限を指定して作る(0 は 1 として扱う)。
    pub fn with_max_entries(max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                records_since_purge: 0,
            }),
            max_entries: max_entries.max(1),
        }
    }

    /// いま表にある行数(テスト・監視用)。
    pub fn len(&self) -> usize {
        self.lock_inner().map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        // 別スレッドが途中で panic しても、数の表は壊れていないので使い続ける
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn digest(key: &str) -> KeyDigest {
        Sha256::digest(key.as_bytes()).into()
    }

    fn purge_expired(map: &mut HashMap<KeyDigest, Bucket>, now: Instant) {
        map.retain(|_, b| b.expires_at() > now);
    }

    /// 満杯のとき、1件追い出す。ロック中でない行のうち最も古いものを選ぶ。
    /// 全部ロック中なら、いちばん早くロックが明ける行を選ぶ。
    fn evict_one(map: &mut HashMap<KeyDigest, Bucket>, now: Instant) {
        let victim = map
            .iter()
            .filter(|(_, b)| !b.is_locked_at(now))
            .min_by_key(|(_, b)| b.window_start)
            .or_else(|| map.iter().min_by_key(|(_, b)| b.expires_at()))
            .map(|(k, _)| *k);
        if let Some(k) = victim {
            map.remove(&k);
        }
    }

    fn record_failure_at(&self, key: &str, cfg: &AttemptLockConfig, now: Instant) {
        let digest = Self::digest(key);
        let mut inner = self.lock_inner();

        inner.records_since_purge += 1;
        if inner.records_since_purge >= PURGE_EVERY {
            inner.records_since_purge = 0;
            Self::purge_expired(&mut inner.map, now);
        }
        if !inner.map.contains_key(&digest) && inner.map.len() >= self.max_entries {
            Self::purge_expired(&mut inner.map, now);
            if inner.map.len() >= self.max_entries {
                Self::evict_one(&mut inner.map, now);
            }
        }

        let bucket = inner.map.entry(digest).or_insert(Bucket {
            failures: 0,
            window_start: now,
            window: cfg.window,
            locked_until: None,
        });

        if let Some(until) = bucket.locked_until {
            if now < until {
                // まだロック中 — 何もしない(ロックを延ばさない)
                return;
            }
            // ロックが明けた → 数え直す
            bucket.locked_until = None;
            bucket.failures = 0;
            bucket.window_start = now;
        }

        // 数える時間窓が過ぎていたら、数え直す
        if now.duration_since(bucket.window_start) >= cfg.window {
            bucket.failures = 0;
            bucket.window_start = now;
        }
        bucket.window = cfg.window;

        bucket.failures = bucket.failures.saturating_add(1);
        if bucket.failures >= cfg.max_attempts {
            bucket.locked_until = Some(now + cfg.lock_duration);
            bucket.failures = 0;
            bucket.window_start = now;
        }
    }

    fn locked_retry_after_at(&self, key: &str, now: Instant) -> Option<u64> {
        let digest = Self::digest(key);
        let mut inner = self.lock_inner();
        let locked_until = inner.map.get(&digest)?.locked_until;
        match locked_until {
            Some(until) if now < until => {
                // 端数は切り上げる(「あと0秒」と返して、すぐ再試行されないように)
                let remaining = until.saturating_duration_since(now);
                let secs = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
                Some(secs.max(1))
            }
            Some(_) => {
                // ロックが明けた行は消す(次の失敗から数え直し)
                inner.map.remove(&digest);
                None
            }
            None => None,
        }
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
        self.record_failure_at(key, cfg, Instant::now());
        Ok(())
    }

    async fn reset(&self, key: &str) -> Result<(), AuthError> {
        let digest = Self::digest(key);
        self.lock_inner().map.remove(&digest);
        Ok(())
    }

    async fn locked_retry_after_seconds(&self, key: &str) -> Result<Option<u64>, AuthError> {
        Ok(self.locked_retry_after_at(key, Instant::now()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(max_attempts: u32, window_secs: u64, lock_secs: u64) -> AttemptLockConfig {
        AttemptLockConfig {
            window: Duration::from_secs(window_secs),
            max_attempts,
            lock_duration: Duration::from_secs(lock_secs),
            ..AttemptLockConfig::default()
        }
    }

    #[tokio::test]
    async fn locks_after_max_attempts_via_trait() {
        let store = InMemoryAttemptStore::new();
        let cfg = AttemptLockConfig::default();
        let key = "198.51.100.1:user@example.com";
        for _ in 0..4 {
            store.record_failure(key, &cfg).await.unwrap();
            assert_eq!(store.locked_retry_after_seconds(key).await.unwrap(), None);
        }
        store.record_failure(key, &cfg).await.unwrap();
        let retry = store
            .locked_retry_after_seconds(key)
            .await
            .unwrap()
            .unwrap();
        assert!((1..=15 * 60).contains(&retry), "retry_after: {retry}");
    }

    #[tokio::test]
    async fn reset_clears_the_count() {
        let store = InMemoryAttemptStore::new();
        let cfg = AttemptLockConfig::default();
        let key = "k";
        for _ in 0..4 {
            store.record_failure(key, &cfg).await.unwrap();
        }
        store.reset(key).await.unwrap();
        for _ in 0..4 {
            store.record_failure(key, &cfg).await.unwrap();
        }
        assert_eq!(store.locked_retry_after_seconds(key).await.unwrap(), None);
        store.record_failure(key, &cfg).await.unwrap();
        assert!(store
            .locked_retry_after_seconds(key)
            .await
            .unwrap()
            .is_some());
    }

    #[test]
    fn failures_outside_the_window_are_not_counted() {
        let store = InMemoryAttemptStore::new();
        let c = cfg(3, 60, 60);
        let t0 = Instant::now();
        store.record_failure_at("k", &c, t0);
        store.record_failure_at("k", &c, t0 + Duration::from_secs(10));
        // 窓(60秒)を過ぎてからの失敗は、1から数え直し
        store.record_failure_at("k", &c, t0 + Duration::from_secs(61));
        store.record_failure_at("k", &c, t0 + Duration::from_secs(62));
        assert_eq!(
            store.locked_retry_after_at("k", t0 + Duration::from_secs(62)),
            None
        );
        store.record_failure_at("k", &c, t0 + Duration::from_secs(63));
        assert!(store
            .locked_retry_after_at("k", t0 + Duration::from_secs(63))
            .is_some());
    }

    #[test]
    fn lock_expires_and_retry_after_counts_down() {
        let store = InMemoryAttemptStore::new();
        let c = cfg(2, 60, 100);
        let t0 = Instant::now();
        store.record_failure_at("k", &c, t0);
        store.record_failure_at("k", &c, t0);
        assert_eq!(store.locked_retry_after_at("k", t0), Some(100));
        assert_eq!(
            store.locked_retry_after_at("k", t0 + Duration::from_millis(30_500)),
            Some(70)
        );
        // ロック中の失敗は、ロックを延ばさない
        store.record_failure_at("k", &c, t0 + Duration::from_secs(50));
        assert_eq!(
            store.locked_retry_after_at("k", t0 + Duration::from_secs(99)),
            Some(1)
        );
        assert_eq!(
            store.locked_retry_after_at("k", t0 + Duration::from_secs(100)),
            None
        );
        // 明けた行は消える
        assert!(store.is_empty());
    }

    #[test]
    fn map_size_is_capped_and_locked_entries_survive() {
        let store = InMemoryAttemptStore::with_max_entries(3);
        let c = cfg(2, 600, 600);
        let t0 = Instant::now();
        // 狙われたアカウントをロックしておく
        store.record_failure_at("victim", &c, t0);
        store.record_failure_at("victim", &c, t0);
        // 攻撃者が毎回違う名前で失敗させても、表は上限を超えない
        for i in 0..1000u64 {
            store.record_failure_at(&format!("junk-{i}"), &c, t0 + Duration::from_millis(i));
        }
        assert!(store.len() <= 3, "len = {}", store.len());
        // ロック中の行は追い出されない
        assert!(store
            .locked_retry_after_at("victim", t0 + Duration::from_secs(1))
            .is_some());
    }

    #[test]
    fn expired_entries_are_purged_when_full() {
        let store = InMemoryAttemptStore::with_max_entries(2);
        let c = cfg(5, 10, 10);
        let t0 = Instant::now();
        store.record_failure_at("a", &c, t0);
        store.record_failure_at("b", &c, t0);
        // 窓が過ぎた後に新しいキーが来たら、期限切れの行が消えて入る
        let later = t0 + Duration::from_secs(11);
        store.record_failure_at("c", &c, later);
        store.record_failure_at("d", &c, later);
        assert_eq!(store.len(), 2);
    }

    #[test]
    fn keys_are_not_kept_in_plain_text() {
        let store = InMemoryAttemptStore::new();
        store.record_failure_at("secret-user@example.com", &cfg(5, 60, 60), Instant::now());
        let dump = format!("{store:?}");
        assert!(!dump.contains("secret-user"));
    }
}
