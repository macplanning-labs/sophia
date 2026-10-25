//! domain/attempt_lock.rs — アカウント単位の試行回数ロック（方針ドキュメント1.2節）
//!
//! IP＋対象キー単位で試行回数を数え、既定では15分に5回失敗すると15分ロックする。
//! ミドルウェア本体（ボディバッファリング・429応答）は
//! `infrastructure::rate_limit::attempt_lock_middleware` にある。ここでは
//! ロック判定に必要なキー抽出トレイトと設定・状態管理を定義する。

use std::time::Duration;

use crate::error::AuthError;

/// ログイン系DTO（フォーム/JSONボディ）が実装するトレイト。
/// ロック判定に使うアカウント側の識別子（例: メールアドレス）を返す。
/// IPと組み合わせて "IP:identifier" のキーを構成する。
pub trait LockKey {
    fn lock_key(&self) -> &str;
}

#[derive(Debug, Clone, Copy)]
pub struct AttemptLockConfig {
    /// 失敗回数をカウントする時間窓
    pub window: Duration,
    /// 何回失敗したらロックするか
    pub max_attempts: u32,
    /// ロック継続時間
    pub lock_duration: Duration,
    /// キー抽出のためにバッファするリクエストボディの上限バイト数。
    /// ログインフォームはメールアドレス+パスワード程度で本来数百バイトあれば
    /// 足りるため、グローバルなDefaultBodyLimitの既定値（例: 2MB）よりも
    /// 大幅に小さい値を既定にする（方針ドキュメント1.2.1節）。
    pub max_body_bytes: usize,
}

impl Default for AttemptLockConfig {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(15 * 60),
            max_attempts: 5,
            lock_duration: Duration::from_secs(15 * 60),
            max_body_bytes: 32 * 1024, // 32KiB（推奨レンジ16〜64KiBの中央値）
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptOutcomeKind {
    Success,
    Failure,
}

/// レスポンスのステータスコードから「試行を成功とみなすか失敗とみなすか」を
/// 判定するトレイト。既定は 401/422→失敗、2xx/3xx→成功（カウンタリセット）。
/// アプリ側で差し替え可能にする。
pub trait AttemptOutcome: Send + Sync {
    fn classify(&self, status: axum::http::StatusCode) -> Option<AttemptOutcomeKind>;
}

pub struct DefaultAttemptOutcome;

impl AttemptOutcome for DefaultAttemptOutcome {
    fn classify(&self, status: axum::http::StatusCode) -> Option<AttemptOutcomeKind> {
        if status.is_success() || status.is_redirection() {
            Some(AttemptOutcomeKind::Success)
        } else if status == axum::http::StatusCode::UNAUTHORIZED
            || status == axum::http::StatusCode::UNPROCESSABLE_ENTITY
        {
            Some(AttemptOutcomeKind::Failure)
        } else {
            None
        }
    }
}

/// 現在のロック・カウント状態の永続化を担うトレイト。auth-coreはメモリ内実装
/// （プロセス内Mutex<HashMap>等）とDB/Redis実装のどちらも想定し、具体的な
/// ストレージには依存しない。
#[async_trait::async_trait]
pub trait AttemptStore: Send + Sync {
    /// キー（"IP:identifier"）の失敗を1件記録する。
    async fn record_failure(&self, key: &str, cfg: &AttemptLockConfig) -> Result<(), AuthError>;

    /// キーのカウンタをリセットする（成功時）。
    async fn reset(&self, key: &str) -> Result<(), AuthError>;

    /// キーが現在ロック中であれば、あと何秒でロックが解除されるかを返す。
    async fn locked_retry_after_seconds(&self, key: &str) -> Result<Option<u64>, AuthError>;
}

/// "IP:identifier" 形式のロックキーを組み立てる。
pub fn build_lock_key(ip: &str, identifier: &str) -> String {
    format!("{ip}:{identifier}")
}

/// 何の単位で失敗を数えるか（DEMO-000153）。
///
/// - `IpAndIdentifier`: 「IP＋アカウント」ごと。少ない回数（既定5回）でロックしてよい。
///   第三者が別の IP から失敗させても、本人の IP の数には影響しないため、
///   ロックを悪用した締め出し（DoS）がしにくい。
/// - `IdentifierOnly`: 「アカウント」ごと（IP を問わない）。IP を次々に変える分散型の総当たりを
///   遅くするためのもの。誰でも本人を締め出せてしまうので、回数は高め（例: 30回）にする。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockScope {
    IpAndIdentifier,
    IdentifierOnly,
}

/// ロック用に識別子をそろえる（前後の空白を除き、小文字にする）。
/// "User@Example.com " と "user@example.com" を同じアカウントとして数えるため。
pub fn normalize_lock_identifier(identifier: &str) -> String {
    identifier.trim().to_lowercase()
}

/// 数える単位（`LockScope`）に合わせてロックキーを組み立てる。識別子はそろえてから使う。
pub fn build_scoped_lock_key(scope: LockScope, ip: &str, identifier: &str) -> String {
    let id = normalize_lock_identifier(identifier);
    match scope {
        LockScope::IpAndIdentifier => build_lock_key(ip, &id),
        LockScope::IdentifierOnly => format!("id:{id}"),
    }
}

impl AttemptLockConfig {
    /// アカウント単位（IP を問わない）向けの既定値: 15分に30回失敗で15分ロック。
    /// 正しい利用者が締め出されにくいよう、IP＋アカウント（5回）より大幅に高くしてある。
    /// IP ごとの率制限（ログインは1分20回）があるため、ここに届くには複数の IP が要る。
    pub fn account_wide_default() -> Self {
        Self {
            max_attempts: 30,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_baseline() {
        let cfg = AttemptLockConfig::default();
        assert_eq!(cfg.max_attempts, 5);
        assert_eq!(cfg.window, Duration::from_secs(15 * 60));
        assert_eq!(cfg.lock_duration, Duration::from_secs(15 * 60));
    }

    #[test]
    fn default_outcome_classifies_unauthorized_as_failure() {
        let outcome = DefaultAttemptOutcome;
        assert_eq!(
            outcome.classify(axum::http::StatusCode::UNAUTHORIZED),
            Some(AttemptOutcomeKind::Failure)
        );
        assert_eq!(
            outcome.classify(axum::http::StatusCode::OK),
            Some(AttemptOutcomeKind::Success)
        );
        assert_eq!(outcome.classify(axum::http::StatusCode::NOT_FOUND), None);
    }

    #[test]
    fn scoped_key_normalizes_identifier() {
        assert_eq!(
            build_scoped_lock_key(
                LockScope::IpAndIdentifier,
                "203.0.113.1",
                "  User@Example.COM "
            ),
            "203.0.113.1:user@example.com"
        );
        assert_eq!(
            build_scoped_lock_key(LockScope::IdentifierOnly, "203.0.113.1", "User@Example.com"),
            "id:user@example.com"
        );
        // アカウント単位のキーは、IP によらず同じ
        assert_eq!(
            build_scoped_lock_key(
                LockScope::IdentifierOnly,
                "198.51.100.7",
                "user@example.com"
            ),
            build_scoped_lock_key(LockScope::IdentifierOnly, "203.0.113.1", "USER@example.com")
        );
    }

    #[test]
    fn account_wide_default_is_higher_than_ip_and_account() {
        let wide = AttemptLockConfig::account_wide_default();
        assert_eq!(wide.max_attempts, 30);
        assert!(wide.max_attempts > AttemptLockConfig::default().max_attempts);
        assert_eq!(wide.window, Duration::from_secs(15 * 60));
        assert_eq!(wide.lock_duration, Duration::from_secs(15 * 60));
    }

    #[test]
    fn lock_key_combines_ip_and_identifier() {
        assert_eq!(
            build_lock_key("203.0.113.1", "user@example.com"),
            "203.0.113.1:user@example.com"
        );
    }
}
