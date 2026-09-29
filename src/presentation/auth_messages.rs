//! presentation/auth_messages.rs — 認証関連の共通レスポンス文言・クライアントIP抽出
//!
//! login_guard.rs（アカウント試行ロックの独自実装）はauth-coreの
//! AttemptStoreパターンへ移行したため削除した。ロックと無関係な汎用ユーティリティ
//! （IP抽出・ユーザー向け文言）だけをここに残す。

use axum::http::HeaderMap;

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

/// Unified login failure message (no user enumeration).
pub const LOGIN_FAIL_MSG: &str = "メールアドレスまたはパスワードが正しくありません";

pub const LOCKED_MSG: &str = "試行回数が上限に達しました。しばらくしてから再度お試しください";
