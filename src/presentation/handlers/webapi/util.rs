/// presentation/handlers/webapi/util.rs — WebAPI ユーティリティ
use axum::http::HeaderMap;
use std::net::SocketAddr;

use crate::presentation::auth_messages::client_ip;

/// クライアント IP アドレスを抽出。送り元の決め方は `auth_messages::client_ip` に一本化している
/// (接続元が信用する中継のときだけ見出しを採用する)。
pub fn extract_client_ip(headers: &HeaderMap, addr: Option<&SocketAddr>) -> String {
    client_ip(addr.copied(), headers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoofed_forwarded_for_is_ignored_without_trusted_proxy() {
        // 環境変数 TRUSTED_PROXY_CIDRS が無い既定では、見出しを信用しない
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.10, 203.0.113.10".parse().unwrap());
        let addr: SocketAddr = "203.0.113.9:4000".parse().unwrap();

        let ip = extract_client_ip(&headers, Some(&addr));
        assert_eq!(ip, "203.0.113.9");
    }

    #[test]
    fn missing_peer_is_unknown() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.10".parse().unwrap());

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, "unknown");
    }
}
