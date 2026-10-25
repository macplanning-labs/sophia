//! 社内 SSO(Cloudflare Access + Keycloak / Google Workspace)でのログイン。DEMO-000233(SOSSO B4)。
//!
//! Cloudflare Access がログイン済みの人のリクエストに付ける JWT を、**署名まで検証**し、そのメールアドレスと
//! 一致する有効な社員・管理者のアカウントに、Sophia のログインの証明(`sophia_jwt` と `sophia_refresh`)を発行する。
//!
//! - JWT の取り出し(ヘッダー優先・Cookie は Origin 完全一致のときだけ)と検証は、部品 `cf-access-jwt` が行う。
//!   **ここに Cookie や Origin の処理を書かない**(P0 / DEMO-000250)。
//! - 対象は社員・管理者だけ。パートナー・エンジニアなど社外の人は、従来のログインのまま(決定 1)。
//! - SSO は IdP が本人確認(パスキー・2 段階認証)を済ませているので、MFA 検証済みとして扱い、
//!   Sophia の MFA の登録は求めない(決定 2・6。JWT に `auth_method = "sso"` を付ける)。
//! - `SSO_ENABLED` が true でなければ 404。従来のログインに影響しない。
//! - DB エラーは握りつぶさず 500(方式設計書 §3.5.4)。該当なし・複数・対象外は 403(理由は監査ログだけ。画面には出さない)。
//! - JWT・Cookie の値はログに出さない。

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Json,
};
use axum_extra::extract::cookie::CookieJar;
use sqlx::PgPool;
use std::sync::LazyLock;
use std::time::Instant;

use crate::config::AppState;
use crate::domain::services::auth_jwt::{self, AUTH_METHOD_SSO};
use crate::domain::services::chat_notifier;
use crate::domain::services::login_role::{is_sso_eligible_role, role_for_login};
use crate::domain::services::sso_monitor::{SsoAlert, SsoMonitor};
use crate::infrastructure::repositories::auth_repo;
use crate::presentation::auth_messages::{client_ip, ClientPeer};
use crate::presentation::cookie_util;

/// SSO の失敗を数える監視(プロセスのメモリ上。ブロックはしない。DEMO-000254 B8)
static SSO_MONITOR: LazyLock<SsoMonitor> = LazyLock::new(SsoMonitor::new);

/// 拒否を 1 件数える。閾値に達したときだけ知らせる(リクエストの扱いは変えない)。
fn note_rejection(ip: &str) {
    for a in SSO_MONITOR.record_rejection(ip, Instant::now()) {
        send_alert(&a);
    }
}

/// 公開鍵の取得失敗を数える。10 分に 1 回だけ知らせる。
fn note_jwks_failure() {
    if let Some(a) = SSO_MONITOR.record_jwks_failure(Instant::now()) {
        send_alert(&a);
    }
}

/// 公開鍵を取得できた。失敗を知らせたあとなら、復旧を 1 回知らせる。
fn note_jwks_ok() {
    if let Some(a) = SSO_MONITOR.record_jwks_ok() {
        send_alert(&a);
    }
}

/// ログ + Chat 通知。通知の失敗でリクエストを遅らせない(別タスクで送る)。
fn send_alert(alert: &SsoAlert) {
    if alert.level() == integration_core::Level::Recovered {
        tracing::info!("[認証/SSO] 処理=復旧の検知 | {}", alert.summary());
    } else {
        tracing::error!("[認証/SSO] 処理=失敗の検知 結果=閾値超過 | {} | {}", alert.summary(), alert.impact());
    }
    let incident = chat_notifier::Incident::new(
        alert.level(),
        alert.summary(),
        alert.impact(),
        alert.action(),
    );
    tokio::spawn(async move {
        chat_notifier::post(&incident).await;
    });
}

/// 画面に出す文言。理由(アカウントなし・複数・対象外・検証失敗)は区別せず、同じ文言にする。
const SSO_DENIED_MSG: &str = "社内アカウントでログインできませんでした。従来のログインをお使いください";

/// 社内 SSO のログインの判定結果(DB を見た結果。JWT の検証は済んでいる前提)。
#[derive(Debug, PartialEq, Eq)]
pub enum SsoLoginOutcome {
    /// このアカウントでログインしてよい。
    Allowed {
        user_id: i64,
        email: String,
        role: &'static str,
    },
    /// 一致する有効なアカウントが無い。
    NoAccount,
    /// 大文字小文字だけが違うなど、一致するアカウントが 2 件以上ある(どれか決められないので入れない)。
    Ambiguous(usize),
    /// アカウントはあるが、社員・管理者ではない(パートナー・エンジニアなど)。
    NotEligible { user_id: i64, role: &'static str },
}

impl SsoLoginOutcome {
    /// 監査ログ(`h_auth_event.detail`)に残す理由。値(メールなど)は含めない。
    fn audit_reason(&self) -> &'static str {
        match self {
            Self::Allowed { .. } => "sso",
            Self::NoAccount => "sso_no_account",
            Self::Ambiguous(_) => "sso_ambiguous_account",
            Self::NotEligible { .. } => "sso_not_eligible_role",
        }
    }
}

/// メールアドレス(小文字)から、ログインしてよいアカウントを決める。DB エラーはそのまま返す(握りつぶさない)。
pub async fn resolve_sso_user(pool: &PgPool, email_lower: &str) -> anyhow::Result<SsoLoginOutcome> {
    let users = auth_repo::find_active_users_by_email_lower(pool, email_lower).await?;
    let (id, email, is_staff, _mfa_enabled) = match users.as_slice() {
        [] => return Ok(SsoLoginOutcome::NoAccount),
        [one] => one.clone(),
        many => return Ok(SsoLoginOutcome::Ambiguous(many.len())),
    };
    // 管理者はプロフィールを見ない。それ以外は employee_id / partner_id で社員・パートナーを分ける。
    let role = if is_staff {
        "ADMIN"
    } else {
        let profile = auth_repo::find_user_profile_role_fields(pool, id).await?;
        role_for_login(false, &profile)
    };
    if !is_sso_eligible_role(role) {
        return Ok(SsoLoginOutcome::NotEligible { user_id: id, role });
    }
    Ok(SsoLoginOutcome::Allowed { user_id: id, email, role })
}

fn json_err(status: StatusCode, msg: &str) -> Response {
    (status, Json(serde_json::json!({ "success": false, "error": msg }))).into_response()
}

/// GET /api/v1/auth/sso/status — 社内 SSO が有効か。
///
/// ログイン画面が「社内アカウントでログイン」のボタンを出すか、自動で試すかを決めるための問い合わせ。
/// 有効かどうかの真偽だけを返す(認証不要。設定の値・JWT・アカウントの情報は含めない)。
pub async fn api_sso_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "enabled": state.sso.is_some() }))
}

/// ログインに成功したときに発行するもの。
struct SsoGrant {
    role: &'static str,
    access: String,
    refresh: String,
}

/// ログインに失敗した理由の種類。画面には出さない(呼び出し側が、JSON の応答か、画面の遷移に変える)。
enum SsoFailure {
    /// SSO が無効なサーバー。
    Disabled,
    /// JWT が無い・検証に失敗した(Cloudflare Access を通っていない)。
    Unauthorized,
    /// JWT は正しいが、入れるアカウントではない(アカウントなし・複数・社外の人)。
    Forbidden,
    /// サーバー側の失敗(DB・トークン発行)。
    Internal,
}

/// 社内 SSO の認証の本体(JWT の取り出し → 検証 → アカウントの決定 → トークンの発行)。
/// POST(画面からの呼び出し)と GET(Access で保護した入口)の両方が使う。監査ログもここで残す。
async fn sso_authenticate(
    state: &AppState,
    headers: &HeaderMap,
    ip: &str,
) -> Result<SsoGrant, SsoFailure> {
    // SSO が無効なら、何もしない(従来のログインだけが使える)
    let Some(sso) = state.sso.as_ref() else {
        return Err(SsoFailure::Disabled);
    };
    let pool = &state.pool;

    // 1. JWT を取り出す(ヘッダー優先。Cookie は Origin 完全一致のときだけ。部品が行う)
    let token = match cf_access_jwt::access_token(headers, &sso.allowed_origins) {
        Ok(t) => t,
        Err(e) => {
            // JWT が無いだけ(Access を通っていない・社外の人がログイン画面を開いただけ)は、ふつうの閲覧。
            // 監査ログを汚さない。Cookie があるのに Origin が合わない、などの不審なものだけ残す。
            // (理由の種類だけを残す。JWT・Cookie の値は出さない)
            if matches!(e, cf_access_jwt::TokenSourceError::Missing) {
                tracing::debug!("SSO ログイン: JWT がありません(Access を通っていない)");
            } else {
                tracing::warn!(reason = %e, "SSO ログイン: JWT を受け取れません");
                auth_repo::insert_auth_event(pool, "login_fail", None, None, Some(ip), "sso_no_token").await;
                note_rejection(ip);
            }
            return Err(SsoFailure::Unauthorized);
        }
    };

    // 2. 署名・iss・aud・期限・メールドメインを検証する(未検証でヘッダーを信じない)
    let principal = match sso.verifier.verify(token).await {
        Ok(p) => {
            note_jwks_ok();
            p
        }
        Err(e) => {
            // 公開鍵を取れない障害(Jwks)は error、それ以外(期限切れ・不正など)は warn
            match &e {
                cf_access_jwt::VerifyError::Jwks(_) => {
                    tracing::error!(error = %e, "SSO ログイン: Access の公開鍵を取得できません");
                    note_jwks_failure();
                }
                _ => {
                    tracing::warn!(error = %e, "SSO ログイン: JWT を拒否しました");
                    note_rejection(ip);
                }
            }
            auth_repo::insert_auth_event(pool, "login_fail", None, None, Some(ip), "sso_invalid_token").await;
            return Err(SsoFailure::Unauthorized);
        }
    };
    // 人だけ。Service Token(機械の通行証)では入れない(設定でも拒否しているが、念のため)
    let cf_access_jwt::Principal::User { email, .. } = principal else {
        tracing::warn!("SSO ログイン: Service Token での通過は受け付けません");
        auth_repo::insert_auth_event(pool, "login_fail", None, None, Some(ip), "sso_service_token").await;
        note_rejection(ip);
        return Err(SsoFailure::Unauthorized);
    };

    // 3. アカウントを決める。DB エラーは握りつぶさず 500(403 に化けさせない)
    let outcome = match resolve_sso_user(pool, &email.to_ascii_lowercase()).await {
        Ok(o) => o,
        Err(e) => {
            tracing::error!("[認証/SSO] 処理=resolve_sso_user 結果=失敗 影響=ログインできない | {e:#}");
            return Err(SsoFailure::Internal);
        }
    };
    let (user_id, user_email, role) = match &outcome {
        SsoLoginOutcome::Allowed { user_id, email, role } => (*user_id, email.clone(), *role),
        other => {
            let user_id = match other {
                SsoLoginOutcome::NotEligible { user_id, .. } => Some(*user_id),
                _ => None,
            };
            if let SsoLoginOutcome::Ambiguous(n) = other {
                tracing::warn!(count = n, "SSO ログイン: 一致するアカウントが複数あるため入れません");
            }
            auth_repo::insert_auth_event(pool, "login_fail", user_id, None, Some(ip), other.audit_reason()).await;
            note_rejection(ip);
            return Err(SsoFailure::Forbidden);
        }
    };

    // 4. 従来のログインと同じ access + refresh を、ログイン方式 SSO つきで発行する。MFA は検証済み扱い
    let pair = match auth_jwt::issue_and_encode_token_pair_with_method(
        user_id,
        role,
        &state.secret_key,
        Some(AUTH_METHOD_SSO),
    ) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[認証/SSO] 処理=トークン発行 結果=失敗 影響=ログインできない | user_id={user_id} {e:?}");
            return Err(SsoFailure::Internal);
        }
    };
    auth_repo::insert_auth_event(pool, "login_success", Some(user_id), Some(&user_email), Some(ip), AUTH_METHOD_SSO).await;
    tracing::info!(user_id, "SSO ログイン");
    Ok(SsoGrant { role, access: pair.access, refresh: pair.refresh })
}

/// POST /api/v1/auth/sso/login — 画面から、すでに Cloudflare Access を通っているブラウザ(CF_Authorization の Cookie を持つ)で呼ぶ。
/// 自動の確認に使う。Access を通っていない(初めて・期限切れ)と 401 になる → 画面は、保護した入口(下の `api_sso_start`)へ移る。
pub async fn api_sso_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: ClientPeer,
    jar: CookieJar,
) -> Response {
    let ip = client_ip(peer.0, &headers);
    match sso_authenticate(&state, &headers, &ip).await {
        Ok(grant) => {
            // access(sophia_jwt)と refresh(sophia_refresh)の両方を Cookie に入れる(方式設計書 §4.3)
            let jar = cookie_util::add_staff_session_cookies(jar, grant.access, grant.refresh);
            (
                jar,
                Json(serde_json::json!({
                    "success": true,
                    "role": grant.role,
                    // 従来のログイン API と同じ形。SSO は MFA の確認も登録の誘導も無い
                    "mfa_required": false,
                    "mfa_setup_required": false
                })),
            )
                .into_response()
        }
        // SSO が無効なら 404(画面は 404 を見て、SSO の案内を出さない)
        Err(SsoFailure::Disabled) => {
            json_err(StatusCode::NOT_FOUND, "社内アカウントでのログインは有効になっていません")
        }
        Err(SsoFailure::Unauthorized) => json_err(StatusCode::UNAUTHORIZED, SSO_DENIED_MSG),
        Err(SsoFailure::Forbidden) => json_err(StatusCode::FORBIDDEN, SSO_DENIED_MSG),
        Err(SsoFailure::Internal) => {
            json_err(StatusCode::INTERNAL_SERVER_ERROR, "ログイン処理に失敗しました")
        }
    }
}

/// `GET /api/v1/auth/sso/start` の問い合わせ。`redirect` は、ログイン後に戻る画面(サイト内の相対パスだけ)。
#[derive(Debug, serde::Deserialize)]
pub struct SsoStartQuery {
    pub redirect: Option<String>,
}

/// ログイン後に戻る先として、安全な相対パスだけを通す。それ以外(外部サイト・`//`・`\`・制御文字・API・長すぎるもの)は `/`。
/// (オープンリダイレクトの対策。画面側の `isSafeRedirect` と同じ考え方を、サーバー側でも確かめる)
pub fn safe_redirect_target(redirect: Option<&str>) -> String {
    match redirect {
        Some(r)
            if r.starts_with('/')
                && !r.starts_with("//")
                && !r.starts_with("/api/")
                && r.len() <= 512
                && !r.contains('\\')
                && !r.chars().any(|c| c.is_control()) =>
        {
            r.to_string()
        }
        _ => "/".to_string(),
    }
}

/// 失敗したときの戻り先。`/login?sso_error=1` に、戻る先が単純なパスなら `redirect` も引き継ぐ(画面で、もう一度試せる)。
fn login_error_location(redirect: Option<&str>) -> String {
    let target = safe_redirect_target(redirect);
    let simple = target != "/"
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '~'));
    if simple {
        format!("/login?sso_error=1&redirect={target}")
    } else {
        "/login?sso_error=1".to_string()
    }
}

/// GET /api/v1/auth/sso/start — **Cloudflare Access で保護する入口**(社内アカウントでのログインの最初の一歩)。
///
/// ログイン画面のボタンが、この URL へ「画面ごと」移動する(`fetch` ではなく)。この 1 本のパスだけを Access の
/// アプリにして(社外の人が使う画面・API は保護しない)、Keycloak(障害時は Google)のログインを通す。通った
/// リクエストには、Access が JWT(`Cf-Access-Jwt-Assertion` ヘッダー)を付けて届ける。ここでそれを署名検証し、
/// Sophia のログインの証明を発行して、元の画面(`redirect`。サイト内のパスだけ)へ戻す。失敗したら `/login?sso_error=1`。
pub async fn api_sso_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: ClientPeer,
    jar: CookieJar,
    Query(q): Query<SsoStartQuery>,
) -> Response {
    let ip = client_ip(peer.0, &headers);
    match sso_authenticate(&state, &headers, &ip).await {
        Ok(grant) => {
            let jar = cookie_util::add_staff_session_cookies(jar, grant.access, grant.refresh);
            (jar, Redirect::to(&safe_redirect_target(q.redirect.as_deref()))).into_response()
        }
        // SSO が無効なサーバーでは、何もせず、ログイン画面へ(画面は SSO を出さない)
        Err(SsoFailure::Disabled) => Redirect::to("/login").into_response(),
        // 理由は区別しない。ログイン画面で、従来のログインか、もう一度の SSO を選べる
        Err(_) => Redirect::to(&login_error_location(q.redirect.as_deref())).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 監査ログの理由は_種類だけで_値を含まない() {
        let cases = [
            (SsoLoginOutcome::NoAccount, "sso_no_account"),
            (SsoLoginOutcome::Ambiguous(2), "sso_ambiguous_account"),
            (SsoLoginOutcome::NotEligible { user_id: 1, role: "PARTNER" }, "sso_not_eligible_role"),
            (
                SsoLoginOutcome::Allowed { user_id: 1, email: "a@b".into(), role: "ADMIN" },
                "sso",
            ),
        ];
        for (o, want) in cases {
            let r = o.audit_reason();
            assert_eq!(r, want);
            assert!(!r.contains('@'), "メールを含めない");
        }
    }

    #[test]
    fn 戻り先は_サイト内の相対パスだけ() {
        assert_eq!(safe_redirect_target(Some("/payroll")), "/payroll");
        assert_eq!(safe_redirect_target(Some("/orders/12?tab=a")), "/orders/12?tab=a");
        assert_eq!(safe_redirect_target(None), "/");
        // 外部サイト・プロトコル相対・バックスラッシュ・制御文字・API・長すぎるものは、すべて `/`
        for bad in [
            "https://evil.example.com",
            "//evil.example.com",
            "/\\evil.example.com",
            "javascript:alert(1)",
            "evil.example.com",
            "",
            "/api/v1/auth/logout",
            "/ok\r\nSet-Cookie: x=1",
            "/ok\0",
        ] {
            assert_eq!(safe_redirect_target(Some(bad)), "/", "{bad:?}");
        }
        assert_eq!(safe_redirect_target(Some(&format!("/{}", "a".repeat(600)))), "/");
    }

    #[test]
    fn 失敗の戻り先は_login_で_単純なパスなら引き継ぐ() {
        assert_eq!(login_error_location(None), "/login?sso_error=1");
        assert_eq!(login_error_location(Some("/payroll")), "/login?sso_error=1&redirect=/payroll");
        // クエリ・記号を含む戻り先、外部、不正は、引き継がない(組み立てた URL に、余計なものを混ぜない)
        assert_eq!(login_error_location(Some("/orders?tab=a&x=1")), "/login?sso_error=1");
        assert_eq!(login_error_location(Some("//evil.example.com")), "/login?sso_error=1");
        assert_eq!(login_error_location(Some("/a b")), "/login?sso_error=1");
        assert_eq!(login_error_location(Some("/")), "/login?sso_error=1");
    }

    #[test]
    fn 画面の文言は_理由を区別しない() {
        // アカウントの有無・複数・対象外を、外から推測させない(どの失敗も同じ文言)
        assert!(!SSO_DENIED_MSG.contains("存在"));
        assert!(!SSO_DENIED_MSG.contains("パートナー"));
    }
}
