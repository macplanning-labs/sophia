//! Session / auth cookie helpers (Secure + SameSite for production HTTPS).

use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

// ── Cookie 名定数 ──

pub const COOKIE_JWT: &str = "sophia_jwt";
pub const COOKIE_REFRESH: &str = "sophia_refresh";
pub const COOKIE_MFA_PENDING: &str = "sophia_mfa_pending";

// ── Max-Age 定数（秒） ──

pub const ACCESS_MAX_AGE_SECS: i64 = 30 * 60; // 30分
pub const REFRESH_MAX_AGE_SECS: i64 = 7 * 24 * 60 * 60; // 7日

/// Prefer Secure cookies when serving over HTTPS (production or BASE_URL=https).
pub fn cookie_secure() -> bool {
    std::env::var("ENV_NAME").unwrap_or_default() == "production"
        || std::env::var("BASE_URL")
            .unwrap_or_default()
            .starts_with("https://")
}

fn cookie_flags_suffix() -> String {
    if cookie_secure() {
        "; Secure; SameSite=Lax".to_string()
    } else {
        "; SameSite=Lax".to_string()
    }
}

/// Set-Cookie value for `sophia_session`.
pub fn sophia_session_header(session_id: &str, max_age_secs: i64) -> String {
    format!(
        "sophia_session={}; HttpOnly; Path=/; Max-Age={}{}",
        session_id,
        max_age_secs,
        cookie_flags_suffix()
    )
}

/// Set-Cookie value for `sophia_jwt`（社員/管理者用アクセストークン, 通常24h）。
/// Step3: bcrypt→Argon2+JWT移行（auth-core）。
pub fn sophia_jwt_header(token: &str, max_age_secs: i64) -> String {
    format!(
        "sophia_jwt={}; HttpOnly; Path=/; Max-Age={}{}",
        token,
        max_age_secs,
        cookie_flags_suffix()
    )
}

/// Set-Cookie value for `sophia_mfa_pending`（MFA未検証状態を表す短命トークン, 通常5分）。
pub fn sophia_mfa_pending_header(token: &str, max_age_secs: i64) -> String {
    format!(
        "sophia_mfa_pending={}; HttpOnly; Path=/; Max-Age={}{}",
        token,
        max_age_secs,
        cookie_flags_suffix()
    )
}

/// Cookie builder with HttpOnly + SameSite (+ Secure when applicable)。
///
/// 1レスポンスで複数Cookieを削除/設定する場合は必ずこちら（`CookieJar::remove`/`add`と組み合わせる）
/// を使うこと。`[(SET_COOKIE, ...); N]`のような同名ヘッダーの配列タプルはaxumが
/// `insert`（後勝ち上書き）で扱うため、2件目以降のSet-Cookieが消えるバグを踏む
/// （Step3 Phase5-2で実際に踏んで修正した）。単一Cookieのみの応答であれば
/// 上記の`*_header`系（Stringを直接Set-Cookieヘッダーに積む方式）でも問題ない。
pub fn build_auth_cookie(name: &'static str, value: String, max_age: Duration) -> Cookie<'static> {
    let mut builder = Cookie::build((name, value))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(max_age);
    if cookie_secure() {
        builder = builder.secure(true);
    }
    builder.build()
}

// ── Staff session Cookie ヘルパー ──

/// access Cookie（30分）を生成
pub fn staff_access_cookie(token: String) -> Cookie<'static> {
    build_auth_cookie(COOKIE_JWT, token, Duration::seconds(ACCESS_MAX_AGE_SECS))
}

/// refresh Cookie（7日）を生成
pub fn staff_refresh_cookie(token: String) -> Cookie<'static> {
    build_auth_cookie(COOKIE_REFRESH, token, Duration::seconds(REFRESH_MAX_AGE_SECS))
}

/// access と refresh Cookie の両方を jar に追加
pub fn add_staff_session_cookies(jar: CookieJar, access: String, refresh: String) -> CookieJar {
    jar.add(staff_access_cookie(access))
        .add(staff_refresh_cookie(refresh))
}

/// access と refresh Cookie の両方を削除
pub fn clear_staff_session_cookies(jar: CookieJar) -> CookieJar {
    jar.remove(build_auth_cookie(COOKIE_JWT, String::new(), Duration::seconds(0)))
        .remove(build_auth_cookie(COOKIE_REFRESH, String::new(), Duration::seconds(0)))
}
