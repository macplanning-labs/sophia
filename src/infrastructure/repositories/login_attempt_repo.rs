//! infrastructure/repositories/login_attempt_repo.rs — ログインの試行の記録と数え方(DB 保存。DEMO-000233 S2-D / 項目6-1)
//!
//! 1 試行 1 行で `s_login_attempt` に記録し、時間窓(`window_secs`)の中の失敗を数える。
//! 規則:
//! - **判定の前に 1 件記録してから数える**(RapidDev の方式。数えたあとに記録すると、同時に大量に送られたとき全部が通るため)。
//!   読み取り時点で、先に記録された行はすべて見えている(INSERT は文ごとにコミット済み)。
//! - 記録のあとの件数が上限(`max`)を超えたら断り、その行は消す(ロック中の試行は記録しない)。
//! - 成功したら、その人の「同じ IP」からの未解除の失敗を無効化する(`succeeded = TRUE`)。別の IP の失敗は残す。
//! - 時刻は DB の `now()` を使う(複数台でもずれない)。

use std::net::{IpAddr, Ipv6Addr};

use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// 保存する試行の種類。混ざらないように、種類ごとに数える
pub const KIND_PASSWORD: &str = "password";
pub const KIND_PORTAL: &str = "portal";
pub const KIND_MFA: &str = "mfa";
/// パスキー(ログインの完了と、パスキーによる二要素認証の完了)の失敗
pub const KIND_PASSKEY: &str = "passkey";
/// パスキーログインの開始(begin)の回数。乱発されやすいので、失敗ではなく開始の回数を数える(上限は緩め)
pub const KIND_PASSKEY_BEGIN: &str = "passkey_begin";
/// パスキーログインの開始を IP 単位で数える上限(15 分あたり)。既存の閾値(5)より緩めにしてある
pub const PASSKEY_BEGIN_MAX: u32 = 30;
/// パートナー向けトークンURLへの無効な試行(存在しない・形式不正)。IP 単位で数える(DEMO-000233 S2-E / 項目7)
pub const KIND_TOKEN: &str = "token";

/// パスキーの試行の鍵(接続元 IP 単位の識別子)。IP が解釈できないときは None(IP 単位は数えない。全員のロックを避ける)
pub fn passkey_ip_key(ip_text: &str, identifier: &str) -> Option<String> {
    parse_ip(ip_text).map(|_| format!("{}:{identifier}", ip_group_text(ip_text)))
}

/// キー(メールなど)を、保存できる形(SHA-256 の 16 進)にする。キーは呼び出し側で正規化してから渡す
pub fn hash_key(key: &str) -> String {
    format!("{:x}", Sha256::digest(key.as_bytes()))
}

/// 前後の空白を除き、小文字にそろえる(アカウントの同一視に使う)
pub fn normalize_account(email: &str) -> String {
    email.trim().to_lowercase()
}

/// IPv6 の「同じ利用者」を束ねる幅(上位 64 ビット)。/64 を 1 つの鍵にすると、同じ /64 の中で
/// アドレスを変えても IP 単位の上限を回避できない(DEMO-000233 S2-D / 項目6-1 の人が決める点)
const IPV6_GROUP_MASK: u128 = !0u128 << 64;

/// 接続元の文字列を IP として解釈する。解釈できないとき(`unknown` など)は None。
/// None の場合は「IP 単位」の数えを行わず、アカウント単位だけで数える。
/// IPv4 を IPv6 の形(`::ffff:a.b.c.d`)で受けたときは、IPv4 として扱う(同じ 1 台を 2 通りに数えない)
pub fn parse_ip(ip: &str) -> Option<IpAddr> {
    match ip.parse::<IpAddr>().ok()? {
        IpAddr::V6(v6) => Some(match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        }),
        v4 => Some(v4),
    }
}

/// IP 単位で数えるときの範囲(CIDR)。IPv4 は 1 台(/32)、IPv6 は /64(同じ /64 は 1 つにまとめる)
pub fn count_scope(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => format!("{v4}/32"),
        IpAddr::V6(v6) => format!("{}/64", Ipv6Addr::from(u128::from(v6) & IPV6_GROUP_MASK)),
    }
}

/// 試行の鍵に入れる IP の表記。IPv6 は /64 の先頭アドレスにそろえる(同じ /64 は同じ鍵になる)。
/// 解釈できないときは、渡された文字列のまま(従来どおり、その文字列で鍵を作る)
pub fn ip_group_text(ip_text: &str) -> String {
    match parse_ip(ip_text) {
        Some(IpAddr::V6(v6)) => Ipv6Addr::from(u128::from(v6) & IPV6_GROUP_MASK).to_string(),
        Some(IpAddr::V4(v4)) => v4.to_string(),
        None => ip_text.to_string(),
    }
}

/// 試行を 1 件記録し、その行の id を返す。`ip` が None のときは NULL で保存する
pub async fn insert_attempt(pool: &PgPool, kind: &str, key_hash: &str, ip: Option<IpAddr>) -> sqlx::Result<i64> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO s_login_attempt (kind, key_hash, ip, succeeded, created_at) \
         VALUES ($1, $2, $3::inet, FALSE, NOW()) RETURNING id",
    )
    .bind(kind)
    .bind(key_hash)
    .bind(ip.map(|i| i.to_string()))
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 時間窓の中の、未解除の失敗の件数。`ip` が Some のときは、その IP の行だけを数える
pub async fn count_failures(
    pool: &PgPool,
    kind: &str,
    key_hash: &str,
    ip: Option<IpAddr>,
    window_secs: i64,
) -> sqlx::Result<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM s_login_attempt \
         WHERE kind = $1 AND key_hash = $2 AND succeeded = FALSE \
           AND ($3::cidr IS NULL OR ip <<= $3::cidr) \
           AND created_at > NOW() - make_interval(secs => $4)",
    )
    .bind(kind)
    .bind(key_hash)
    .bind(ip.map(count_scope))
    .bind(window_secs as f64)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// 件数が上限まで減るまでの秒数。新しい順に `max` 件目の行が時間窓から抜ける時刻を求める。
/// 上限を超えている(件数 >= max)ときに使う。行が無いときは None
pub async fn lock_remaining_secs(
    pool: &PgPool,
    kind: &str,
    key_hash: &str,
    ip: Option<IpAddr>,
    window_secs: i64,
    max: i64,
) -> sqlx::Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT CEIL(EXTRACT(EPOCH FROM (created_at + make_interval(secs => $4) - NOW())))::BIGINT \
         FROM s_login_attempt \
         WHERE kind = $1 AND key_hash = $2 AND succeeded = FALSE \
           AND ($3::cidr IS NULL OR ip <<= $3::cidr) \
           AND created_at > NOW() - make_interval(secs => $4) \
         ORDER BY created_at DESC OFFSET $5 LIMIT 1",
    )
    .bind(kind)
    .bind(key_hash)
    .bind(ip.map(count_scope))
    .bind(window_secs as f64)
    .bind((max - 1).max(0))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(secs,)| secs.max(1)))
}

/// 成功: 同じキー・同じ IP の未解除の失敗を無効化する(`ip` が None のときは、NULL の行だけ)。
/// 別の IP の行は残す(RapidDev の教訓。成功した IP だけ数えを戻す)
pub async fn invalidate_failures(pool: &PgPool, kind: &str, key_hash: &str, ip: Option<IpAddr>) -> sqlx::Result<u64> {
    let r = sqlx::query(
        "UPDATE s_login_attempt SET succeeded = TRUE \
         WHERE kind = $1 AND key_hash = $2 AND succeeded = FALSE \
           AND (($3::cidr IS NULL AND ip IS NULL) OR ($3::cidr IS NOT NULL AND ip <<= $3::cidr))",
    )
    .bind(kind)
    .bind(key_hash)
    .bind(ip.map(count_scope))
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// 記録した 1 件を消す(ロック中の試行を、記録しないことにする)
pub async fn delete_attempt(pool: &PgPool, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM s_login_attempt WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 保存期間(7 日)を過ぎた行を、古い順に一定件数ずつ消す。消した件数を返す
pub async fn purge_expired(pool: &PgPool, limit: i64) -> sqlx::Result<u64> {
    let r = sqlx::query(
        "DELETE FROM s_login_attempt WHERE id IN ( \
           SELECT id FROM s_login_attempt \
           WHERE created_at < NOW() - INTERVAL '7 days' \
           ORDER BY created_at LIMIT $1)",
    )
    .bind(limit)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// パスワードログインの判定の結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reservation {
    /// 試行してよい。`id` は記録した行(成功・失敗の後始末に使う)
    Allowed { id: i64 },
    /// ロック中。`retry_after_secs` 秒後に再試行できる。記録はしていない
    Locked { retry_after_secs: u64 },
}

/// アカウント単位と「IP + アカウント」単位の上限(`ACCOUNT_MAX` / `IP_MAX`)と時間窓
pub const ACCOUNT_MAX: i64 = 30;
pub const IP_MAX: i64 = 5;
pub const LOCK_WINDOW_SECS: i64 = 15 * 60;
/// 無効なトークンの試行の上限(IP 単位、15 分あたり)。超えたら 429。有効なトークンの利用は数えない
pub const TOKEN_INVALID_MAX: i64 = 20;
/// トークンの試行の鍵。IP だけで数えるため、全部の試行で同じ鍵を使う(IP の範囲で分ける)
const TOKEN_INVALID_KEY: &str = "invalid-token";

/// パスワードログインの試行を予約する。1 件記録し、アカウント単位と IP 単位の件数を数える。
/// どちらかが上限を超えていたら、記録を消して `Locked` を返す。
pub async fn reserve_password_attempt(pool: &PgPool, account_key: &str, ip: Option<IpAddr>) -> sqlx::Result<Reservation> {
    let key_hash = hash_key(account_key);
    let id = insert_attempt(pool, KIND_PASSWORD, &key_hash, ip).await?;

    let account_count = count_failures(pool, KIND_PASSWORD, &key_hash, None, LOCK_WINDOW_SECS).await?;
    let ip_count = match ip {
        Some(_) => count_failures(pool, KIND_PASSWORD, &key_hash, ip, LOCK_WINDOW_SECS).await?,
        None => 0,
    };

    let account_over = account_count > ACCOUNT_MAX;
    let ip_over = ip.is_some() && ip_count > IP_MAX;
    if !account_over && !ip_over {
        return Ok(Reservation::Allowed { id });
    }

    // ロック中の試行は記録しない(解除時刻を延ばさないため、自分の 1 件を消す)
    delete_attempt(pool, id).await?;
    let mut retry: Option<i64> = None;
    if account_over {
        retry = retry.max(lock_remaining_secs(pool, KIND_PASSWORD, &key_hash, None, LOCK_WINDOW_SECS, ACCOUNT_MAX).await?);
    }
    if ip_over {
        retry = retry.max(lock_remaining_secs(pool, KIND_PASSWORD, &key_hash, ip, LOCK_WINDOW_SECS, IP_MAX).await?);
    }
    Ok(Reservation::Locked {
        retry_after_secs: retry.unwrap_or(1).max(1) as u64,
    })
}

/// 無効なトークンの試行を 1 件数える(IP 単位)。上限を超えていれば、記録を消して `Locked` を返す。
/// `ip` が解釈できないときは呼ばない(呼び出し側で数えない。全員のロックを避ける)。
pub async fn reserve_invalid_token_attempt(pool: &PgPool, ip: IpAddr) -> sqlx::Result<Reservation> {
    let key_hash = hash_key(TOKEN_INVALID_KEY);
    let id = insert_attempt(pool, KIND_TOKEN, &key_hash, Some(ip)).await?;
    let count = count_failures(pool, KIND_TOKEN, &key_hash, Some(ip), LOCK_WINDOW_SECS).await?;
    if count <= TOKEN_INVALID_MAX {
        return Ok(Reservation::Allowed { id });
    }
    delete_attempt(pool, id).await?;
    let secs = lock_remaining_secs(pool, KIND_TOKEN, &key_hash, Some(ip), LOCK_WINDOW_SECS, TOKEN_INVALID_MAX).await?;
    Ok(Reservation::Locked {
        retry_after_secs: secs.unwrap_or(1).max(1) as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_key_is_stable_and_hides_the_raw_key() {
        let a = hash_key("user@example.com");
        assert_eq!(a, hash_key("user@example.com"));
        assert_eq!(a.len(), 64);
        assert!(!a.contains("example"));
        assert_ne!(a, hash_key("other@example.com"));
    }

    #[test]
    fn account_is_normalized_before_hashing() {
        assert_eq!(
            hash_key(&normalize_account("  User@Example.COM ")),
            hash_key(&normalize_account("user@example.com"))
        );
    }

    #[test]
    fn unknown_client_ip_is_not_counted_per_ip() {
        assert_eq!(parse_ip("unknown"), None);
        assert_eq!(parse_ip(""), None);
        assert!(parse_ip("203.0.113.1").is_some());
        assert!(parse_ip("2001:db8::1").is_some());
    }

    #[test]
    fn ipv6_is_counted_per_64_bit_prefix() {
        let a: IpAddr = "2001:db8:1:2:aaaa:bbbb:cccc:dddd".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2::1".parse().unwrap();
        let other_net: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(count_scope(a), "2001:db8:1:2::/64");
        assert_eq!(count_scope(a), count_scope(b), "同じ /64 は同じ範囲");
        assert_ne!(count_scope(a), count_scope(other_net));
        assert_eq!(ip_group_text("2001:db8:1:2:aaaa:bbbb:cccc:dddd"), "2001:db8:1:2::");
        assert_eq!(ip_group_text("2001:db8:1:2::1"), ip_group_text("2001:db8:1:2:ffff::9"));
    }

    #[test]
    fn ipv4_is_counted_per_address_and_mapped_form_is_ipv4() {
        assert_eq!(count_scope("203.0.113.10".parse().unwrap()), "203.0.113.10/32");
        assert_eq!(ip_group_text("203.0.113.10"), "203.0.113.10");
        // IPv4 を IPv6 の形で受けても、IPv4 の 1 台として数える
        assert_eq!(parse_ip("::ffff:203.0.113.10"), parse_ip("203.0.113.10"));
        assert_eq!(ip_group_text("::ffff:203.0.113.10"), "203.0.113.10");
        assert_eq!(passkey_ip_key("unknown", "login"), None);
        assert_eq!(passkey_ip_key("203.0.113.10", "login").as_deref(), Some("203.0.113.10:login"));
    }

    #[test]
    fn limits_match_the_agreed_values() {
        assert_eq!(ACCOUNT_MAX, 30);
        assert_eq!(IP_MAX, 5);
        assert_eq!(LOCK_WINDOW_SECS, 900);
        assert_eq!(TOKEN_INVALID_MAX, 20);
    }
}
