//! domain/attempt_window.rs — 二要素認証の試行の数え方(ユーザー単位・時間窓・ロック)の純粋な計算(DEMO-000153 A-1)
//!
//! 数を DB に保存して、行ロックで直列化する部分は、テーブルを持つ各アプリが書く。ここには、**DB に依存しない計算**だけを置く
//! (SENN と Sophia が、同じ規則で数えるため。試験もここで持つ)。
//!
//! 規則: 時間窓(`window`)の間に調べてよい回数は `max_attempts` まで。超えた回の試行は断り、その人の確認を `window` の間止める。
//! 調べる**前**に 1 回予約する(調べたあとに数えると、同時に大量に送られたとき、数える前に全部が通るため)。

use chrono::{DateTime, Duration, Utc};

/// 試行の予約の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptReservation {
    /// コードを調べてよい
    Allowed,
    /// ロック中(コードを調べずに断る)
    Locked,
}

/// 保存されている状態と現在時刻から、次の状態を決める。
/// 戻り値: (新しい attempts, 新しい window_start, 新しい locked_until, 結果)
pub fn next_attempt_state(
    attempts: i32,
    window_start: DateTime<Utc>,
    locked_until: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    max_attempts: i32,
    window: Duration,
) -> (
    i32,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
    AttemptReservation,
) {
    if let Some(until) = locked_until {
        if until > now {
            return (
                attempts,
                window_start,
                Some(until),
                AttemptReservation::Locked,
            );
        }
    }
    // 時間窓が過ぎた(ロックが明けた場合も含む)ら、数え直す
    let (mut attempts, window_start) = if window_start + window <= now || locked_until.is_some() {
        (0, now)
    } else {
        (attempts, window_start)
    };
    attempts += 1;
    if attempts > max_attempts {
        (
            attempts,
            window_start,
            Some(now + window),
            AttemptReservation::Locked,
        )
    } else {
        (attempts, window_start, None, AttemptReservation::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(sec: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000 + sec, 0).unwrap()
    }
    const W: i64 = 900;

    #[test]
    fn counts_up_then_locks_on_the_attempt_after_the_limit() {
        let w = Duration::seconds(W);
        let (mut a, mut ws, mut lk) = (0, t(0), None);
        for i in 1..=10 {
            let (na, nws, nlk, r) = next_attempt_state(a, ws, lk, t(i), 10, w);
            assert_eq!(r, AttemptReservation::Allowed, "{i} 回目");
            (a, ws, lk) = (na, nws, nlk);
        }
        let (_, _, lk, r) = next_attempt_state(a, ws, lk, t(11), 10, w);
        assert_eq!(r, AttemptReservation::Locked);
        assert_eq!(lk, Some(t(11) + w));
    }

    #[test]
    fn locked_stays_locked_and_does_not_extend() {
        let w = Duration::seconds(W);
        let until = t(500);
        let (a, _, lk, r) = next_attempt_state(11, t(0), Some(until), t(100), 10, w);
        assert_eq!((a, lk, r), (11, Some(until), AttemptReservation::Locked));
    }

    #[test]
    fn window_expiry_and_lock_expiry_restart_the_count() {
        let w = Duration::seconds(W);
        let (a, ws, lk, r) = next_attempt_state(9, t(0), None, t(W + 1), 10, w);
        assert_eq!(
            (a, ws, lk, r),
            (1, t(W + 1), None, AttemptReservation::Allowed)
        );
        let (a, _, lk, r) = next_attempt_state(11, t(0), Some(t(W)), t(W + 5), 10, w);
        assert_eq!((a, lk, r), (1, None, AttemptReservation::Allowed));
    }
}
