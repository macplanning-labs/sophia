//! リクエストから、検証する前の JWT を取り出す。検証は [`crate::Verifier::verify`] が行う。
//!
//! JWT はまず `Cf-Access-Jwt-Assertion` ヘッダーから探す。ヘッダーが無いときは、
//! Access のログイン Cookie(`CF_Authorization`)を同じ JWT として使う。
//! `/api/` などを Access の Bypass にしているアプリでは、Bypass の経路にヘッダーが付かないため。
//!
//! Cookie はブラウザが自動で送るので、別のサイトから送られた要求でも付いてくる。
//! そのため Cookie を使うのは、`Origin` が許可したオリジンと**完全に一致**するときだけにする
//! (`Origin` が無い・空・別のオリジン・前方一致だけ、は使わない)。

use axum::http::{header, HeaderMap};

use crate::HEADER_NAME;

/// Access のログイン Cookie の名前。
pub const COOKIE_NAME: &str = "CF_Authorization";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenSourceError {
    /// ヘッダーにも Cookie にも JWT が無い。
    #[error("JWT がありません(ヘッダー・Cookie とも)")]
    Missing,
    /// Cookie に JWT はあるが、`Origin` が許可したオリジンと一致しないため使わない。
    #[error("Cookie の JWT は、許可したオリジンのページからの送信だけ受け付けます")]
    CookieOriginNotAllowed,
}

/// 検証する JWT を取り出す(検証はまだしない)。ヘッダーを優先し、無ければ Cookie を使う。
///
/// `allowed_origins` は、このアプリ自身のオリジン(例: `https://app.example.com`)。末尾の `/` は無視する。
/// **空の要素は無視する**(`BASE_URL` の未設定などで空文字が混ざっても、`Origin` が空の要求を通さない)。
pub fn access_token<'a>(
    headers: &'a HeaderMap,
    allowed_origins: &[String],
) -> Result<&'a str, TokenSourceError> {
    if let Some(v) = headers
        .get(HEADER_NAME)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        return Ok(v);
    }

    let cookie = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE_NAME)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
        .ok_or(TokenSourceError::Missing)?;

    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|o| o.trim().trim_end_matches('/'))
        .filter(|o| !o.is_empty());
    match origin {
        Some(o)
            if allowed_origins
                .iter()
                .map(|a| a.trim().trim_end_matches('/'))
                .filter(|a| !a.is_empty())
                .any(|a| a == o) =>
        {
            Ok(cookie)
        }
        _ => Err(TokenSourceError::CookieOriginNotAllowed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    const SELF: &str = "https://app.example.com";

    fn hdrs(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.append(*k, HeaderValue::from_str(v).unwrap());
        }
        h
    }

    fn allowed() -> Vec<String> {
        vec![SELF.to_string()]
    }

    #[test]
    fn ヘッダーを優先する() {
        let h = hdrs(&[
            ("cf-access-jwt-assertion", "H"),
            ("cookie", "CF_Authorization=C"),
        ]);
        assert_eq!(access_token(&h, &allowed()), Ok("H"));
    }

    #[test]
    fn ヘッダーが空白だけなら_cookie_を使う() {
        let h = hdrs(&[
            ("cf-access-jwt-assertion", "   "),
            ("cookie", "CF_Authorization=C"),
            ("origin", SELF),
        ]);
        assert_eq!(access_token(&h, &allowed()), Ok("C"));
    }

    #[test]
    fn ヘッダーなら_origin_が無くても通す() {
        let h = hdrs(&[("cf-access-jwt-assertion", "H")]);
        assert_eq!(access_token(&h, &allowed()), Ok("H"));
    }

    #[test]
    fn ヘッダーが無ければ_自分のページからの_cookie_を使う() {
        let h = hdrs(&[("cookie", "a=1; CF_Authorization=C; b=2"), ("origin", SELF)]);
        assert_eq!(access_token(&h, &[format!("{SELF}/")]), Ok("C"));
    }

    #[test]
    fn cookie_が複数の_cookie_ヘッダーに分かれていても拾う() {
        let h = hdrs(&[
            ("cookie", "a=1"),
            ("cookie", "CF_Authorization=C"),
            ("origin", SELF),
        ]);
        assert_eq!(access_token(&h, &allowed()), Ok("C"));
    }

    #[test]
    fn 許可したオリジンの末尾のスラッシュは無視する() {
        let h = hdrs(&[("cookie", "CF_Authorization=C"), ("origin", SELF)]);
        assert_eq!(access_token(&h, &[format!("{SELF}/")]), Ok("C"));
        let h = hdrs(&[
            ("cookie", "CF_Authorization=C"),
            ("origin", &format!("{SELF}/")),
        ]);
        assert_eq!(access_token(&h, &allowed()), Ok("C"));
    }

    #[test]
    fn 別サイトの_origin_の_cookie_は使わない() {
        let h = hdrs(&[
            ("cookie", "CF_Authorization=C"),
            ("origin", "https://evil.example"),
        ]);
        assert_eq!(
            access_token(&h, &allowed()),
            Err(TokenSourceError::CookieOriginNotAllowed)
        );
    }

    #[test]
    fn origin_が無い_cookie_は使わない() {
        let h = hdrs(&[("cookie", "CF_Authorization=C")]);
        assert_eq!(
            access_token(&h, &allowed()),
            Err(TokenSourceError::CookieOriginNotAllowed)
        );
    }

    #[test]
    fn 前方一致だけの_origin_は使わない() {
        for origin in [
            "https://app.example.com.evil.example",
            "https://app.example.com:8443",
            "http://app.example.com",
            "https://app.example.comx",
        ] {
            let h = hdrs(&[("cookie", "CF_Authorization=C"), ("origin", origin)]);
            assert_eq!(
                access_token(&h, &allowed()),
                Err(TokenSourceError::CookieOriginNotAllowed),
                "{origin}"
            );
        }
    }

    #[test]
    fn 空の_origin_は_許可リストに空文字が混ざっていても使わない() {
        // BASE_URL が未設定で、許可リストに空文字が入ってしまった場合の事故を防ぐ。
        let allowed_with_empty = vec![String::new(), "  ".to_string(), SELF.to_string()];
        for origin in ["", " ", "/"] {
            let h = hdrs(&[("cookie", "CF_Authorization=C"), ("origin", origin)]);
            assert_eq!(
                access_token(&h, &allowed_with_empty),
                Err(TokenSourceError::CookieOriginNotAllowed),
                "origin={origin:?}"
            );
        }
        // 許可リストが空(または空文字だけ)なら、どのオリジンも使わない。
        let h = hdrs(&[("cookie", "CF_Authorization=C"), ("origin", SELF)]);
        assert_eq!(
            access_token(&h, &[]),
            Err(TokenSourceError::CookieOriginNotAllowed)
        );
        assert_eq!(
            access_token(&h, &[String::new()]),
            Err(TokenSourceError::CookieOriginNotAllowed)
        );
    }

    #[test]
    fn cookie_の値が空なら_無いものとして扱う() {
        let h = hdrs(&[("cookie", "CF_Authorization=; x=1"), ("origin", SELF)]);
        assert_eq!(access_token(&h, &allowed()), Err(TokenSourceError::Missing));
    }

    #[test]
    fn どちらも無ければ_失敗() {
        assert_eq!(
            access_token(&HeaderMap::new(), &allowed()),
            Err(TokenSourceError::Missing)
        );
        let similar = hdrs(&[("cookie", "CF_AuthorizationX=C"), ("origin", SELF)]);
        assert_eq!(
            access_token(&similar, &allowed()),
            Err(TokenSourceError::Missing)
        );
    }

    #[test]
    fn エラーの文言に_jwt_の値を含めない() {
        let h = hdrs(&[
            ("cookie", "CF_Authorization=secret-token-value"),
            ("origin", "https://evil.example"),
        ]);
        let msg = access_token(&h, &allowed()).unwrap_err().to_string();
        assert!(!msg.contains("secret-token-value"));
    }
}
