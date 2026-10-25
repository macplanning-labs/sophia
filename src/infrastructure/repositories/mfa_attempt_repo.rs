//! infrastructure/repositories/mfa_attempt_repo.rs — 二要素認証の試行の数え方(ユーザー単位・DB 保存)
//!
//! DEMO-000153 S-1(SENN の A-1・A-2 と同じ規則。計算の部分は共通部品 `auth-core` の `attempt_window`)。
//! 確認を調べる**前**に、試行を 1 回予約する。調べたあとに数えると、同時に大量に送られたとき、
//! 数える前に全部が通ってしまう。行をロック(FOR UPDATE)して、予約を直列にするので、並列に送っても、
//! 上限を超えて調べることはできない。時刻は DB の `now()` を使う(複数台でもずれない)。

use anyhow::Result;
use auth_core::domain::attempt_window::{next_attempt_state, AttemptReservation};
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

/// 時間窓の間に、確認を調べてよい回数(超えた回で止める)と、時間窓・ロックの長さ(分)。運用を見て調整する
pub const MFA_MAX_ATTEMPTS: i32 = 10;
pub const MFA_LOCK_WINDOW_MINUTES: i64 = 15;

/// 予約の結果。ロック中のときは、あと何秒で明けるか(`Retry-After` 用)も返す
pub struct Reservation {
    pub outcome: AttemptReservation,
    pub retry_after_secs: Option<i64>,
}

pub async fn reserve_attempt(pool: &PgPool, user_id: i64, max_attempts: i32, window: Duration) -> Result<Reservation> {
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO s_mfa_user_attempt (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    let (attempts, window_start, locked_until, now): (i32, DateTime<Utc>, Option<DateTime<Utc>>, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT attempts, window_start, locked_until, now() FROM s_mfa_user_attempt WHERE user_id = $1 FOR UPDATE",
        )
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    let (attempts, window_start, locked_until, outcome) =
        next_attempt_state(attempts, window_start, locked_until, now, max_attempts, window);
    sqlx::query("UPDATE s_mfa_user_attempt SET attempts = $2, window_start = $3, locked_until = $4 WHERE user_id = $1")
        .bind(user_id)
        .bind(attempts)
        .bind(window_start)
        .bind(locked_until)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let retry_after_secs = match (outcome, locked_until) {
        (AttemptReservation::Locked, Some(until)) => Some((until - now).num_seconds().max(1)),
        _ => None,
    };
    Ok(Reservation { outcome, retry_after_secs })
}

/// 確認が正しかったとき、数を消す
pub async fn reset_attempts(pool: &PgPool, user_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM s_mfa_user_attempt WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 期限切れの行を、古い順に一定件数ずつ消す
pub async fn purge_expired(pool: &PgPool, window: Duration, limit: i64) -> Result<u64> {
    let r = sqlx::query(
        "DELETE FROM s_mfa_user_attempt WHERE user_id IN ( \
           SELECT user_id FROM s_mfa_user_attempt \
           WHERE locked_until IS NULL AND window_start < now() - make_interval(secs => $1) \
           ORDER BY window_start LIMIT $2)",
    )
    .bind(window.num_seconds() as f64)
    .bind(limit)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// 使った区間を記録する。**それより新しい区間のときだけ**記録でき、同じ区間・古い区間なら false(使用済み)。
/// 1 回の UPDATE なので、同時に同じコードを送られても 1 回しか通らない
pub async fn mark_totp_step_used(pool: &PgPool, device_id: i32, step: i64) -> Result<bool> {
    let r = sqlx::query(
        "UPDATE s_totp_device SET last_used_step = $2 WHERE id = $1 AND (last_used_step IS NULL OR last_used_step < $2)",
    )
    .bind(device_id)
    .bind(step)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() == 1)
}
