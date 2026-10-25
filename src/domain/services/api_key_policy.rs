/// domain/services/api_key_policy.rs — 外部向けWebAPIのAPIキーの有効期限の判定
///
/// 期限は任意(`NULL` = 無期限)。既存のキーはすべて無期限のまま動く。DEMO-000167 項目7。

use chrono::{DateTime, Duration, Utc};

/// 発行時に指定できる有効日数の上限(10年)
pub const MAX_EXPIRY_DAYS: u32 = 3650;

/// 期限切れか。`None`(無期限)は期限切れにならない。期限ちょうどの時刻から無効。
pub fn is_expired(expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match expires_at {
        Some(t) => now >= t,
        None => false,
    }
}

/// 発行時の `expires_in_days` を、期限の時刻にする。省略(`None`)は無期限(`Ok(None)`)。
/// 0 と上限超過は不正。
pub fn expiry_from_days(days: Option<u32>, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, &'static str> {
    match days {
        None => Ok(None),
        Some(0) => Err("expires_in_days must be 1 or more"),
        Some(d) if d > MAX_EXPIRY_DAYS => Err("expires_in_days must be 3650 or less"),
        Some(d) => Ok(Some(now + Duration::days(d as i64))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn 期限なしは期限切れにならない() {
        assert!(!is_expired(None, now()));
        assert!(!is_expired(None, now() + Duration::days(36500)));
    }

    #[test]
    fn 期限内は有効_期限切れは無効() {
        let t = now();
        assert!(!is_expired(Some(t + Duration::days(1)), t));
        assert!(is_expired(Some(t - Duration::days(1)), t));
    }

    #[test]
    fn 期限ちょうどから無効() {
        let t = now();
        assert!(is_expired(Some(t), t));
        assert!(!is_expired(Some(t + Duration::seconds(1)), t));
    }

    #[test]
    fn 日数が省略されたら無期限() {
        assert_eq!(expiry_from_days(None, now()), Ok(None));
    }

    #[test]
    fn 日数から期限を作る() {
        let t = now();
        assert_eq!(expiry_from_days(Some(1), t), Ok(Some(t + Duration::days(1))));
        assert_eq!(expiry_from_days(Some(90), t), Ok(Some(t + Duration::days(90))));
        assert_eq!(expiry_from_days(Some(MAX_EXPIRY_DAYS), t), Ok(Some(t + Duration::days(3650))));
    }

    #[test]
    fn 日数が0や上限超過は不正() {
        assert!(expiry_from_days(Some(0), now()).is_err());
        assert!(expiry_from_days(Some(MAX_EXPIRY_DAYS + 1), now()).is_err());
        assert!(expiry_from_days(Some(u32::MAX), now()).is_err());
    }
}
