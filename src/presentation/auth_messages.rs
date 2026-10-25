//! presentation/auth_messages.rs — 認証関連の共通レスポンス文言・クライアントIP抽出
//!
//! login_guard.rs（アカウント試行ロックの独自実装）はauth-coreの
//! AttemptStoreパターンへ移行したため削除した。ロックと無関係な汎用ユーティリティ
//! （IP抽出・ユーザー向け文言）だけをここに残す。
//!
//! 送り元 IP の決め方は、この中の `client_ip` 1 か所に集める(DEMO-000233 S2-C / 項目6-2)。
//! 接続元(peer)が信用する中継(環境変数 `TRUSTED_PROXY_CIDRS`。既定は「なし」)のときだけ、
//! 中継が書いたヘッダ(X-Real-IP、無ければ X-Forwarded-For の末尾)を採用する。
//! 判定の本体は auth-core の `resolve_client_ip`(固定リビジョン 504ae7f)。

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::sync::OnceLock;

use auth_core::infrastructure::client_ip::{
    resolve_client_ip, TrustedProxies, TRUSTED_PROXY_CIDRS_ENV,
};
use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::{request::Parts, Extensions, HeaderMap, Request};
use tower_governor::{errors::GovernorError, key_extractor::KeyExtractor};

/// 信用する中継の既定値。何も信用しない(安全側)。nginx 等の範囲は環境変数で指定する。
const TRUSTED_PROXY_DEFAULT: &str = "none";

/// 起動後に 1 回だけ環境変数 `TRUSTED_PROXY_CIDRS` を読む。
fn trusted_proxies() -> &'static TrustedProxies {
    static TRUSTED: OnceLock<TrustedProxies> = OnceLock::new();
    TRUSTED.get_or_init(|| {
        let spec = std::env::var(TRUSTED_PROXY_CIDRS_ENV)
            .unwrap_or_else(|_| TRUSTED_PROXY_DEFAULT.to_string());
        TrustedProxies::parse(&spec)
    })
}

/// 起動時に `TRUSTED_PROXY_CIDRS` を整える(DEMO-000233 S2-C)。
///
/// auth-core の試行ロック(`attempt_lock_middleware`)は、この環境変数が「未設定・空」のとき
/// 私人網を信用する。一方このファイルは「なし(none)」を既定にする。読む側が 2 つあると、
/// 同じ送り元が食い違うので、起動時に「none」へそろえる。
///
/// - 値があれば `Ok(None)`(何もしない)。`none` と明示した場合もこれ。
/// - 未設定・空で、本番判定(`production` / `dr` / `staging`)なら `Err`(起動を止める)。
/// - 未設定・空で、それ以外(local など)なら `Ok(Some("none"))`(呼び出し側が環境変数へ入れる)。
///
/// 値はログにも戻り値にも出さない(項目名だけ)。
pub fn trusted_proxy_fallback(
    env_name: &str,
    current: Option<&str>,
) -> Result<Option<&'static str>, String> {
    if current.is_some_and(|v| !v.trim().is_empty()) {
        return Ok(None);
    }
    if matches!(env_name, "production" | "dr" | "staging") {
        return Err(format!(
            "{TRUSTED_PROXY_CIDRS_ENV} が未設定または空です（ENV_NAME={env_name}）。\
             nginx の範囲を設定してください。何も信用しないなら none と明示してください"
        ));
    }
    Ok(Some(TRUSTED_PROXY_DEFAULT))
}

/// 本番系で、権限判定(AUTHZ_MODE)が強制(enforce)で、AUTHZ_ENFORCE_DOMAINS に `*` があることを確かめる(DEMO-000254)。
///
/// 未設定・shadow・off のままだと、権限の無い社員が他人の請求書などを読めてしまう(影モードは記録だけで拒否しない)。
/// 対象は `production` / `dr` / `staging`。それ以外(local など)は何もしない。
/// 判定の基準は `AuthzConfig::from_values`(authz.rs)と同じ(前後の空白と大文字小文字を無視する)。
/// 値は秘密ではないので、エラーには項目名と必要な値を書く。
pub fn authz_enforce_startup_check(
    env_name: &str,
    mode: Option<&str>,
    domains: Option<&str>,
) -> Result<(), String> {
    if !matches!(env_name, "production" | "dr" | "staging") {
        return Ok(());
    }
    if !mode.is_some_and(|m| m.trim().eq_ignore_ascii_case("enforce")) {
        return Err(format!(
            "AUTHZ_MODE が enforce ではありません（ENV_NAME={env_name}）。enforce に設定してください。\
             未設定や shadow のままだと、権限の無い操作が拒否されません（DEMO-000254）"
        ));
    }
    if !domains.is_some_and(|d| d.split(',').any(|s| s.trim() == "*")) {
        return Err(format!(
            "AUTHZ_ENFORCE_DOMAINS に * がありません（ENV_NAME={env_name}）。* を設定してください。未設定・空も失敗します"
        ));
    }
    Ok(())
}

/// 送り元 IP(文字列)。アプリ内で送り元を決める唯一の入口。
///
/// - `peer` は接続元(`ConnectInfo`)。本番の起動(`serve`)では必ず入る。無いときは配線の不具合なので
///   `tracing::error!` を出し、「unknown」を返す(偽装されうる見出しは見ない)。
/// - 接続元が信用する中継でないときは、見出しを一切見ず接続元の IP を使う。
pub fn client_ip(peer: Option<SocketAddr>, headers: &HeaderMap) -> String {
    client_ip_with(peer, headers, trusted_proxies())
}

/// `client_ip` の本体(信用する中継の一覧を引数で受ける。試験で使う)。
fn client_ip_with(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    trusted: &TrustedProxies,
) -> String {
    match peer {
        Some(addr) => resolve_client_ip(addr.ip(), headers, trusted).to_string(),
        None => {
            tracing::error!("接続元(ConnectInfo)を取得できませんでした。送り元を unknown として扱います（配線の不具合の疑い）");
            "unknown".to_string()
        }
    }
}

fn peer_from_extensions(ext: &Extensions) -> Option<SocketAddr> {
    ext.get::<ConnectInfo<SocketAddr>>().map(|c| c.0)
}

/// リクエストの接続元(`into_make_service_with_connect_info` で入る)。ミドルウェア用。
pub fn peer_of<B>(req: &Request<B>) -> Option<SocketAddr> {
    peer_from_extensions(req.extensions())
}

/// ハンドラの引数に書く接続元。取れなくても拒否しない(`None`。`client_ip` が unknown にする)。
#[derive(Debug, Clone, Copy)]
pub struct ClientPeer(pub Option<SocketAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientPeer {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(peer_from_extensions(&parts.extensions)))
    }
}

/// 回数制限(tower_governor)の送り元 IP の決め方。`SmartIpKeyExtractor` の代わりに使う。
/// `client_ip` と同じ判定(信用する中継からだけ見出しを採用)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SophiaIpKeyExtractor;

impl KeyExtractor for SophiaIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        // 接続元が無ければ、回数制限の鍵を作れないので拒否する(fail-closed。unknown に束ねない)。
        let peer = peer_of(req).ok_or_else(|| {
            tracing::error!("接続元(ConnectInfo)を取得できず、回数制限の鍵を作れません（配線の不具合の疑い）");
            GovernorError::UnableToExtractKey
        })?;
        Ok(resolve_client_ip(
            peer.ip(),
            req.headers(),
            trusted_proxies(),
        ))
    }
}

/// Unified login failure message (no user enumeration).
pub const LOGIN_FAIL_MSG: &str = "メールアドレスまたはパスワードが正しくありません";

pub const LOCKED_MSG: &str = "試行回数が上限に達しました。しばらくしてから再度お試しください";

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn sock(s: &str) -> Option<SocketAddr> {
        Some(s.parse().unwrap())
    }

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn trusted_nginx() -> TrustedProxies {
        TrustedProxies::parse("172.16.0.0/12,127.0.0.1/32,::1/128")
    }

    #[test]
    fn untrusted_peer_ignores_spoofed_headers() {
        // 信用しない接続元(インターネット直結など)は、偽の見出しで送り元を変えられない
        let h = headers(&[
            ("cf-connecting-ip", "1.1.1.1"),
            ("x-forwarded-for", "2.2.2.2"),
            ("x-real-ip", "3.3.3.3"),
        ]);
        assert_eq!(
            client_ip_with(sock("203.0.113.9:4000"), &h, &trusted_nginx()),
            "203.0.113.9"
        );
    }

    #[test]
    fn default_none_trusts_nobody() {
        let h = headers(&[("x-real-ip", "198.51.100.7")]);
        let none = TrustedProxies::parse("none");
        assert_eq!(
            client_ip_with(sock("172.18.0.5:4000"), &h, &none),
            "172.18.0.5"
        );
        assert_eq!(
            client_ip_with(sock("127.0.0.1:4000"), &h, &none),
            "127.0.0.1"
        );
    }

    #[test]
    fn trusted_peer_uses_x_real_ip() {
        let h = headers(&[
            ("x-real-ip", "198.51.100.7"),
            ("x-forwarded-for", "1.2.3.4"),
        ]);
        assert_eq!(
            client_ip_with(sock("172.18.0.5:4000"), &h, &trusted_nginx()),
            "198.51.100.7"
        );
    }

    #[test]
    fn trusted_peer_without_real_ip_uses_last_forwarded() {
        // 先頭は送り手が書けるので使わない。中継が足した末尾を使う
        let h = headers(&[("x-forwarded-for", "9.9.9.9, 198.51.100.7")]);
        assert_eq!(
            client_ip_with(sock("172.18.0.5:4000"), &h, &trusted_nginx()),
            "198.51.100.7"
        );
    }

    #[test]
    fn multiple_forwarded_lines_use_the_very_last_value() {
        let h = headers(&[
            ("x-forwarded-for", "1.1.1.1"),
            ("x-forwarded-for", "198.51.100.7"),
        ]);
        assert_eq!(
            client_ip_with(sock("172.18.0.5:4000"), &h, &trusted_nginx()),
            "198.51.100.7"
        );
    }

    #[test]
    fn spoofed_first_values_do_not_change_the_key() {
        // 先頭の値を毎回変えても、送り元は同じ(回数制限をすり抜けられない)
        let t = trusted_nginx();
        let a = client_ip_with(
            sock("172.18.0.5:1"),
            &headers(&[("x-forwarded-for", "1.1.1.1, 198.51.100.7")]),
            &t,
        );
        let b = client_ip_with(
            sock("172.18.0.5:1"),
            &headers(&[("x-forwarded-for", "2.2.2.2, 198.51.100.7")]),
            &t,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn invalid_values_fall_back_to_peer() {
        let h = headers(&[("x-real-ip", "not-an-ip"), ("x-forwarded-for", "garbage")]);
        assert_eq!(
            client_ip_with(sock("172.18.0.5:4000"), &h, &trusted_nginx()),
            "172.18.0.5"
        );
    }

    #[test]
    fn ipv6_peer_and_ipv6_header() {
        let h = headers(&[("x-real-ip", "2001:db8::7")]);
        assert_eq!(
            client_ip_with(sock("[::1]:4000"), &h, &trusted_nginx()),
            "2001:db8::7"
        );
        // 信用しない IPv6 の接続元は見出しを見ない
        assert_eq!(
            client_ip_with(sock("[2001:db8::99]:4000"), &h, &trusted_nginx()),
            "2001:db8::99"
        );
    }

    #[test]
    fn ipv4_mapped_ipv6_peer_is_treated_as_ipv4() {
        let h = headers(&[("x-real-ip", "198.51.100.7")]);
        assert_eq!(
            client_ip_with(sock("[::ffff:172.18.0.5]:4000"), &h, &trusted_nginx()),
            "198.51.100.7"
        );
    }

    #[test]
    fn missing_peer_is_unknown_and_ignores_headers() {
        let h = headers(&[("x-real-ip", "198.51.100.7")]);
        assert_eq!(client_ip_with(None, &h, &trusted_nginx()), "unknown");
    }

    #[test]
    fn extractor_key_follows_the_same_rule() {
        let req = Request::builder()
            .header("x-real-ip", "198.51.100.7")
            .body(())
            .unwrap();
        let mut req = req;
        req.extensions_mut().insert(ConnectInfo::<SocketAddr>(
            "203.0.113.9:4000".parse().unwrap(),
        ));
        // 既定(環境変数なし)では何も信用しないので、接続元の IP になる
        let key = SophiaIpKeyExtractor.extract(&req).unwrap();
        assert_eq!(key, "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn extractor_without_peer_fails_closed() {
        let req = Request::builder().body(()).unwrap();
        assert!(SophiaIpKeyExtractor.extract(&req).is_err());
    }

    #[test]
    fn startup_keeps_an_explicit_value() {
        assert_eq!(trusted_proxy_fallback("production", Some("172.16.0.0/12")), Ok(None));
        assert_eq!(trusted_proxy_fallback("local", Some("none")), Ok(None));
    }

    #[test]
    fn startup_fails_in_production_like_envs_when_unset_or_blank() {
        for env in ["production", "dr", "staging"] {
            assert!(trusted_proxy_fallback(env, None).is_err(), "{env} unset");
            assert!(trusted_proxy_fallback(env, Some("  ")).is_err(), "{env} blank");
        }
    }

    #[test]
    fn startup_sets_none_outside_production_when_unset_or_blank() {
        assert_eq!(trusted_proxy_fallback("local", None), Ok(Some("none")));
        assert_eq!(trusted_proxy_fallback("local", Some("")), Ok(Some("none")));
    }

    #[test]
    fn fallback_value_trusts_nobody_for_both_readers() {
        // 起動時に入れる「none」は、auth-core の from_env と Sophia の既定で同じ意味になる
        let t = TrustedProxies::parse(TRUSTED_PROXY_DEFAULT);
        assert!(!t.contains(ip_for_test("172.18.0.5")));
        assert!(!t.contains(ip_for_test("127.0.0.1")));
    }

    #[test]
    fn authz_check_skips_non_production_envs() {
        assert_eq!(authz_enforce_startup_check("local", None, None), Ok(()));
        assert_eq!(authz_enforce_startup_check("test", Some("shadow"), None), Ok(()));
    }

    #[test]
    fn authz_check_requires_enforce_mode_in_production_like_envs() {
        for env in ["production", "dr", "staging"] {
            for mode in [None, Some(""), Some("shadow"), Some("off")] {
                let r = authz_enforce_startup_check(env, mode, Some("*"));
                assert!(r.is_err(), "{env} mode {mode:?}");
                assert!(r.unwrap_err().contains("AUTHZ_MODE"), "{env} message names the setting");
            }
            assert_eq!(authz_enforce_startup_check(env, Some("enforce"), Some("*")), Ok(()));
        }
    }

    #[test]
    fn authz_check_accepts_enforce_regardless_of_case_and_spaces() {
        assert_eq!(authz_enforce_startup_check("production", Some(" Enforce "), Some("*")), Ok(()));
    }

    #[test]
    fn authz_check_requires_star_in_enforce_domains() {
        for domains in [None, Some(""), Some("  "), Some("settings,partner"), Some("*x")] {
            let r = authz_enforce_startup_check("staging", Some("enforce"), domains);
            assert!(r.is_err(), "domains {domains:?}");
            assert!(r.unwrap_err().contains("AUTHZ_ENFORCE_DOMAINS"), "message names the setting");
        }
        assert_eq!(authz_enforce_startup_check("dr", Some("enforce"), Some("settings, *")), Ok(()));
    }

    fn ip_for_test(s: &str) -> std::net::IpAddr {
        s.parse().unwrap()
    }
}
