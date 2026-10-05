//! domain/one_time_token.rs — 汎用ワンタイムトークン（方針ドキュメント5章）
//!
//! Sophiaのパートナー向けメール送付トークン（UUIDv4、24時間有効）や
//! スマホ連携用QRコードトークン（同パターン、10分有効）が実装している
//! 「ランダムトークン発行→期限内検証→使用済みマーク」というパターンを
//! 汎用化したもの。ユーザーモデルに依存しない純粋なプリミティブ。
//!
//! Step 1時点ではSophia側の実コードを参照できなかったため、方針ドキュメント
//! 5章に記載のパターン記述から新規スケルトンとして起こした。Step 3
//! （Sophia適用）でSophiaの既存実装と突き合わせて調整することを想定する。

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AuthError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneTimeToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

impl OneTimeToken {
    /// 新しいワンタイムトークンを発行する（UUIDv4、指定TTL）。
    pub fn issue(ttl: Duration) -> Self {
        Self {
            token: Uuid::new_v4().to_string(),
            expires_at: Utc::now() + ttl,
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// ワンタイムトークンの永続化を担うトレイト。auth-coreはDBを知らないため、
/// 「発行時に保存する」「検証時にCAS的UPDATEで使用済みマークする」処理は
/// アプリ側の実装に委ねる。`consume`は「見つかって・期限内で・未使用なら
/// 使用済みにして成功」を単一操作で行うこと（TOCTOU回避のため、
/// アプリ側はDBの`UPDATE ... WHERE token = $1 AND used_at IS NULL AND expires_at > now()`
/// のような条件付き更新で実装する）。
#[async_trait::async_trait]
pub trait OneTimeTokenStore: Send + Sync {
    async fn save(&self, token: &OneTimeToken, subject_id: &str) -> Result<(), AuthError>;

    /// トークンを検証し、成功した場合は紐づく`subject_id`を返しつつ使用済みにする。
    /// 既に使用済み・期限切れ・存在しない場合は`Ok(None)`を返す。
    async fn consume(&self, token: &str) -> Result<Option<String>, AuthError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_token_is_not_expired_immediately() {
        let token = OneTimeToken::issue(Duration::hours(24));
        assert!(!token.is_expired());
    }

    #[test]
    fn zero_ttl_token_is_expired() {
        let token = OneTimeToken::issue(Duration::seconds(-1));
        assert!(token.is_expired());
    }
}
