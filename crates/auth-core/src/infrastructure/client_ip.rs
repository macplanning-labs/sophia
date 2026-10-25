//! infrastructure/client_ip.rs — 送り元(クライアント)IP の確定
//!
//! 回数制限(`rate_limit`)のキーに使う送り元 IP を決める。
//!
//! 以前は `tower_governor` の `SmartIpKeyExtractor` を使っていたが、これは
//! X-Forwarded-For の**先頭**を、接続元が誰であっても信用する。X-Forwarded-For の先頭は
//! 送り手が自由に書けるので、毎回ちがう値を書けば IP の回数制限をすり抜けられた
//! (DEMO-000144)。
//!
//! ここでは次の順で決める。
//! 1. 接続元(peer)が「信用する中継」(`TRUSTED_PROXY_CIDRS`)の中にいなければ、接続元の IP
//!    をそのまま使う。見出し(ヘッダー)は一切見ない
//! 2. 信用する中継から来たときだけ、中継が書いた値を使う
//!    - `X-Real-IP`(nginx が `$remote_addr` で上書きして渡す)
//!    - 無ければ X-Forwarded-For の**末尾**(中継が自分で見た接続元を足す位置)。先頭は使わない
//! 3. どちらも無ければ接続元の IP
//!
//! `TRUSTED_PROXY_CIDRS` の既定値は、ループバックとプライベート網(同じ Docker の
//! ネットワークにいる nginx を想定)。`none` で中継を信用しない(見出しを全く見ない)。

use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;

use axum::http::{HeaderMap, Request};
use tower_governor::{errors::GovernorError, key_extractor::KeyExtractor};

/// 信用する中継の既定値。nginx などの中継が、アプリと同じ機械か同じ Docker の
/// ネットワークにいる構成を想定する。
pub const DEFAULT_TRUSTED_PROXY_CIDRS: &str =
    "127.0.0.0/8,::1/128,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,fc00::/7";

/// 信用する中継を指定する環境変数の名前。カンマ区切りの CIDR(例: `172.16.0.0/12,10.0.0.5/32`)。
pub const TRUSTED_PROXY_CIDRS_ENV: &str = "TRUSTED_PROXY_CIDRS";

/// IP の範囲(CIDR)。外部の部品を増やさないよう、必要な分だけ自前で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IpCidr {
    network: IpAddr,
    prefix: u8,
}

impl IpCidr {
    /// `"172.16.0.0/12"` や `"::1"`(長さを省くと 1 つの IP)を読む。
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        let (addr, prefix) = match s.split_once('/') {
            Some((a, p)) => (a.parse::<IpAddr>().ok()?, Some(p.parse::<u8>().ok()?)),
            None => (s.parse::<IpAddr>().ok()?, None),
        };
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max);
        if prefix > max {
            return None;
        }
        Some(Self {
            network: addr,
            prefix,
        })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, canonical(ip)) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                prefix_matches(&net.octets(), &ip.octets(), self.prefix)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                prefix_matches(&net.octets(), &ip.octets(), self.prefix)
            }
            _ => false,
        }
    }
}

fn prefix_matches(net: &[u8], ip: &[u8], prefix: u8) -> bool {
    let full = (prefix / 8) as usize;
    if net[..full] != ip[..full] {
        return false;
    }
    let rest = prefix % 8;
    if rest == 0 {
        return true;
    }
    let mask = 0xffu8 << (8 - rest);
    (net[full] & mask) == (ip[full] & mask)
}

/// `::ffff:192.0.2.1` のような IPv4 を包んだ IPv6 を、IPv4 にそろえる。
fn canonical(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map(IpAddr::V4)
            .unwrap_or(IpAddr::V6(v6)),
        v4 => v4,
    }
}

/// 信用する中継の一覧。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrustedProxies(Vec<IpCidr>);

impl TrustedProxies {
    /// カンマ区切りの CIDR を読む。読めない値は警告して捨てる。`none` は空(誰も信用しない)。
    pub fn parse(spec: &str) -> Self {
        if spec.trim().eq_ignore_ascii_case("none") {
            return Self(Vec::new());
        }
        let list = spec
            .split(',')
            .filter(|s| !s.trim().is_empty())
            .filter_map(|s| {
                let parsed = IpCidr::parse(s);
                if parsed.is_none() {
                    tracing::warn!(
                        value = s.trim(),
                        "TRUSTED_PROXY_CIDRS の値を読めないため無視します"
                    );
                }
                parsed
            })
            .collect();
        Self(list)
    }

    /// 環境変数 `TRUSTED_PROXY_CIDRS` から読む。無ければ既定値。
    pub fn from_env() -> Self {
        match std::env::var(TRUSTED_PROXY_CIDRS_ENV) {
            Ok(v) if !v.trim().is_empty() => Self::parse(&v),
            _ => Self::parse(DEFAULT_TRUSTED_PROXY_CIDRS),
        }
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        self.0.iter().any(|c| c.contains(ip))
    }
}

/// 起動後に 1 回だけ環境変数を読む。
fn global_trusted_proxies() -> &'static TrustedProxies {
    static TRUSTED: OnceLock<TrustedProxies> = OnceLock::new();
    TRUSTED.get_or_init(TrustedProxies::from_env)
}

const X_REAL_IP: &str = "x-real-ip";
const X_FORWARDED_FOR: &str = "x-forwarded-for";

/// 接続元 IP と見出しから、送り元 IP を決める(本体。テストしやすいよう純粋な関数にしている)。
pub fn resolve_client_ip(peer: IpAddr, headers: &HeaderMap, trusted: &TrustedProxies) -> IpAddr {
    let peer = canonical(peer);
    if !trusted.contains(peer) {
        return peer;
    }
    let real_ip = headers
        .get(X_REAL_IP)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<IpAddr>().ok());
    if let Some(ip) = real_ip {
        return canonical(ip);
    }
    // 末尾 = 中継が自分で足した値。先頭は送り手が書けるので使わない。
    let last_forwarded = headers
        .get_all(X_FORWARDED_FOR)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(','))
        .filter_map(|s| s.trim().parse::<IpAddr>().ok())
        .next_back();
    last_forwarded.map(canonical).unwrap_or(peer)
}

/// 回数制限のキーを決める部品。`SmartIpKeyExtractor` の代わりに使う。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrustedProxyIpKeyExtractor;

impl KeyExtractor for TrustedProxyIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        let peer = req
            .extensions()
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .or_else(|| req.extensions().get::<SocketAddr>().map(|a| a.ip()))
            .ok_or(GovernorError::UnableToExtractKey)?;
        Ok(resolve_client_ip(
            peer,
            req.headers(),
            global_trusted_proxies(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn default_trusted() -> TrustedProxies {
        TrustedProxies::parse(DEFAULT_TRUSTED_PROXY_CIDRS)
    }

    #[test]
    fn untrusted_peer_ignores_all_headers() {
        // インターネットから直接つながった送り手が、見出しで IP を偽っても効かない
        let h = headers(&[("x-forwarded-for", "1.2.3.4"), ("x-real-ip", "5.6.7.8")]);
        assert_eq!(
            resolve_client_ip(ip("203.0.113.9"), &h, &default_trusted()),
            ip("203.0.113.9")
        );
    }

    #[test]
    fn trusted_proxy_uses_x_real_ip() {
        let h = headers(&[
            ("x-real-ip", "198.51.100.7"),
            ("x-forwarded-for", "1.2.3.4"),
        ]);
        assert_eq!(
            resolve_client_ip(ip("172.18.0.5"), &h, &default_trusted()),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn trusted_proxy_uses_last_forwarded_not_first() {
        // DEMO-000144: 先頭は送り手が書いた偽物。中継が足した末尾を使う
        let h = headers(&[("x-forwarded-for", "9.9.9.9, 198.51.100.7")]);
        assert_eq!(
            resolve_client_ip(ip("172.18.0.5"), &h, &default_trusted()),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn spoofed_first_values_do_not_change_the_key() {
        // 先頭の値を毎回変えても、キーは同じ(=回数制限をすり抜けられない)
        let t = default_trusted();
        let a = resolve_client_ip(
            ip("172.18.0.5"),
            &headers(&[("x-forwarded-for", "1.1.1.1, 198.51.100.7")]),
            &t,
        );
        let b = resolve_client_ip(
            ip("172.18.0.5"),
            &headers(&[("x-forwarded-for", "2.2.2.2, 198.51.100.7")]),
            &t,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn multiple_forwarded_headers_use_the_very_last() {
        let h = headers(&[
            ("x-forwarded-for", "1.1.1.1"),
            ("x-forwarded-for", "198.51.100.7"),
        ]);
        assert_eq!(
            resolve_client_ip(ip("10.0.0.2"), &h, &default_trusted()),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn trusted_proxy_without_headers_falls_back_to_peer() {
        assert_eq!(
            resolve_client_ip(ip("127.0.0.1"), &HeaderMap::new(), &default_trusted()),
            ip("127.0.0.1")
        );
    }

    #[test]
    fn unparsable_headers_fall_back_to_peer() {
        let h = headers(&[("x-real-ip", "not-an-ip"), ("x-forwarded-for", "garbage")]);
        assert_eq!(
            resolve_client_ip(ip("172.18.0.5"), &h, &default_trusted()),
            ip("172.18.0.5")
        );
    }

    #[test]
    fn none_trusts_nobody() {
        let t = TrustedProxies::parse("none");
        let h = headers(&[("x-real-ip", "198.51.100.7")]);
        assert_eq!(resolve_client_ip(ip("127.0.0.1"), &h, &t), ip("127.0.0.1"));
    }

    #[test]
    fn ipv4_mapped_ipv6_peer_is_treated_as_ipv4() {
        let h = headers(&[("x-real-ip", "198.51.100.7")]);
        assert_eq!(
            resolve_client_ip(ip("::ffff:172.18.0.5"), &h, &default_trusted()),
            ip("198.51.100.7")
        );
    }

    #[test]
    fn cidr_parse_and_contains() {
        let c = IpCidr::parse("172.16.0.0/12").unwrap();
        assert!(c.contains(ip("172.31.255.255")));
        assert!(!c.contains(ip("172.32.0.0")));
        assert!(IpCidr::parse("::1").unwrap().contains(ip("::1")));
        assert!(IpCidr::parse("fc00::/7").unwrap().contains(ip("fd12::1")));
        assert!(IpCidr::parse("10.0.0.0/33").is_none());
        assert!(IpCidr::parse("nope").is_none());
        assert!(IpCidr::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
    }

    #[test]
    fn invalid_entries_are_skipped() {
        let t = TrustedProxies::parse("bogus, 10.1.2.3/32");
        assert!(t.contains(ip("10.1.2.3")));
        assert!(!t.contains(ip("10.1.2.4")));
    }

    #[test]
    fn public_addresses_are_not_trusted_by_default() {
        let t = default_trusted();
        assert!(!t.contains(ip("203.0.113.9")));
        assert!(!t.contains(ip("2001:db8::1")));
        assert!(t.contains(ip("172.20.1.5")));
        assert!(t.contains(ip("10.1.2.3")));
    }
}
