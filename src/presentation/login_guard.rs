//! Login / MFA / portal-login rate limiting and lockout (in-memory).
//!
//! Goals (P5-1d):
//! - Slow online guessing when Cloudflare Access is intentionally off
//! - Stop user enumeration via distinct login error messages
//!
//! Limits (per key):
//! - 5 failures within 15 minutes → lock for 15 minutes
//! - Successful auth clears the key

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::http::HeaderMap;

const MAX_FAILURES: u32 = 5;
const WINDOW: Duration = Duration::from_secs(15 * 60);
const LOCKOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
struct Bucket {
    failures: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

fn store() -> &'static Mutex<HashMap<String, Bucket>> {
    static STORE: OnceLock<Mutex<HashMap<String, Bucket>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Prefer Cloudflare / proxy headers when behind Tunnel.
pub fn client_ip(headers: &HeaderMap) -> String {
    if let Some(v) = headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("CF-Connecting-IP"))
        .and_then(|h| h.to_str().ok())
    {
        return v.trim().to_string();
    }
    if let Some(v) = headers.get("x-forwarded-for").and_then(|h| h.to_str().ok()) {
        if let Some(first) = v.split(',').next() {
            let ip = first.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    if let Some(v) = headers.get("x-real-ip").and_then(|h| h.to_str().ok()) {
        let ip = v.trim();
        if !ip.is_empty() {
            return ip.to_string();
        }
    }
    "unknown".to_string()
}

pub fn login_key(ip: &str, email: &str) -> String {
    format!("login:{}:{}", ip, email.trim().to_lowercase())
}

pub fn portal_key(ip: &str, email: &str) -> String {
    format!("portal:{}:{}", ip, email.trim().to_lowercase())
}

pub fn mfa_key(ip: &str, user_id: i64) -> String {
    format!("mfa:{}:{}", ip, user_id)
}

/// Unified login failure message (no user enumeration).
pub const LOGIN_FAIL_MSG: &str = "メールアドレスまたはパスワードが正しくありません";

pub const LOCKED_MSG: &str = "試行回数が上限に達しました。しばらくしてから再度お試しください";

#[derive(Debug, PartialEq, Eq)]
pub enum GuardStatus {
    Allowed,
    Locked { retry_after_secs: u64 },
}

fn now() -> Instant {
    Instant::now()
}

/// Returns lock status without mutating the bucket (except expiry cleanup).
pub fn check(key: &str) -> GuardStatus {
    let mut map = store().lock().unwrap_or_else(|e| e.into_inner());
    let Some(bucket) = map.get_mut(key) else {
        return GuardStatus::Allowed;
    };
    let t = now();
    if let Some(until) = bucket.locked_until {
        if t < until {
            return GuardStatus::Locked {
                retry_after_secs: until.saturating_duration_since(t).as_secs().max(1),
            };
        }
        bucket.locked_until = None;
        bucket.failures = 0;
        bucket.window_start = t;
    }
    if t.duration_since(bucket.window_start) > WINDOW {
        bucket.failures = 0;
        bucket.window_start = t;
    }
    GuardStatus::Allowed
}

pub fn record_failure(key: &str) -> GuardStatus {
    let mut map = store().lock().unwrap_or_else(|e| e.into_inner());
    let t = now();
    let bucket = map.entry(key.to_string()).or_insert_with(|| Bucket {
        failures: 0,
        window_start: t,
        locked_until: None,
    });

    if let Some(until) = bucket.locked_until {
        if t < until {
            return GuardStatus::Locked {
                retry_after_secs: until.saturating_duration_since(t).as_secs().max(1),
            };
        }
        bucket.locked_until = None;
        bucket.failures = 0;
        bucket.window_start = t;
    }

    if t.duration_since(bucket.window_start) > WINDOW {
        bucket.failures = 0;
        bucket.window_start = t;
    }

    bucket.failures = bucket.failures.saturating_add(1);
    if bucket.failures >= MAX_FAILURES {
        bucket.locked_until = Some(t + LOCKOUT);
        bucket.failures = 0;
        bucket.window_start = t;
        return GuardStatus::Locked {
            retry_after_secs: LOCKOUT.as_secs(),
        };
    }
    GuardStatus::Allowed
}

pub fn record_success(key: &str) {
    let mut map = store().lock().unwrap_or_else(|e| e.into_inner());
    map.remove(key);
}

/// Test helper: clear all buckets.
#[cfg(test)]
pub fn reset_for_test() {
    let mut map = store().lock().unwrap_or_else(|e| e.into_inner());
    map.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn locks_after_five_failures() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();
        let key = "login:test:user@example.com";
        for _ in 0..4 {
            assert_eq!(record_failure(key), GuardStatus::Allowed);
        }
        match record_failure(key) {
            GuardStatus::Locked { retry_after_secs } => {
                assert!(retry_after_secs > 0);
            }
            GuardStatus::Allowed => panic!("expected lock"),
        }
        assert!(matches!(check(key), GuardStatus::Locked { .. }));
    }

    #[test]
    fn success_clears_failures() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_for_test();
        let key = "login:test:ok@example.com";
        for _ in 0..3 {
            record_failure(key);
        }
        record_success(key);
        assert_eq!(check(key), GuardStatus::Allowed);
    }
}
