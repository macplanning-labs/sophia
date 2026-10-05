/// presentation/handlers/webapi/util.rs — WebAPI ユーティリティ

use axum::http::HeaderMap;
use std::net::SocketAddr;

/// クライアント IP アドレスを抽出（X-Forwarded-For → X-Real-IP → SocketAddr）
pub fn extract_client_ip(headers: &HeaderMap, addr: Option<&SocketAddr>) -> String {
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            if let Some(first_ip) = s.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(s) = real_ip.to_str() {
            return s.to_string();
        }
    }

    if let Some(socket_addr) = addr {
        return socket_addr.ip().to_string();
    }

    "unknown".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_client_ip_x_forwarded_for() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.10, 203.0.113.10".parse().unwrap());

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, "203.0.113.10");
    }

    #[test]
    fn test_extract_client_ip_x_real_ip() {
        let mut headers = HeaderMap::new();
        headers.insert("x-real-ip", "203.0.113.10".parse().unwrap());

        let ip = extract_client_ip(&headers, None);
        assert_eq!(ip, "203.0.113.10");
    }
}
