//! domain/audit.rs — 認証監査ログ（方針ドキュメント5章）
//!
//! イベント定義はauth-core、永続化はアプリ側（監査イベント用テーブル等）。
//! auth-coreは適切なタイミングで`AuthAuditSink`を
//! 呼ぶだけで、テーブルスキーマは持たない。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthEvent {
    LoginSuccess,
    LoginFail,
    MfaSuccess,
    MfaFail,
    Logout,
    PasswordReset,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditContext {
    /// アプリ側のユーザー識別子（文字列化して渡す。型はアプリ依存のため）
    pub subject_id: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub occurred_at: DateTime<Utc>,
    /// アプリ固有の追加情報（例: 対象ロール、失敗理由等）
    pub extra: serde_json::Value,
}

impl AuditContext {
    pub fn new(subject_id: Option<String>) -> Self {
        Self {
            subject_id,
            ip_address: None,
            user_agent: None,
            occurred_at: Utc::now(),
            extra: serde_json::Value::Null,
        }
    }
}

#[async_trait::async_trait]
pub trait AuthAuditSink: Send + Sync {
    async fn record(&self, event: AuthEvent, ctx: AuditContext);
}

/// テスト・開発用: 標準ログ(`tracing`)に出力するだけのSink実装。
pub struct TracingAuditSink;

#[async_trait::async_trait]
impl AuthAuditSink for TracingAuditSink {
    async fn record(&self, event: AuthEvent, ctx: AuditContext) {
        tracing::info!(?event, subject_id = ?ctx.subject_id, ip = ?ctx.ip_address, "auth event");
    }
}
