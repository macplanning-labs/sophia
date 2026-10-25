//! presentation/middleware/token_guard.rs — トークンURLの試行制限と監査(DEMO-000233 S2-E / S2-E2 / 項目7)
//!
//! 対象(種類ごとに `Family`):
//! - `Partner`: `/api/v1/token/{uuid}[/op]` と `/token/{uuid}[/op]`(注文書・支払通知書・請求書の画面・PDF 直リンク)
//! - `Invite`: `/api/v1/invite/{uuid}/{status,accept,register/begin,register/complete}`(エンジニア招待)
//! - `PortalLink`: `/api/v1/auth/portal-link/{token}[/confirm]`(技術者ポータルの魔法リンク)
//! - `MobileUpload`: `/api/v1/mobile-upload/{token}[/status]` と `/api/mobile-upload/{token}[/status]`(経費の携帯アップロード)
//!
//! 順序(リクエストごと):
//! 1. トークンが存在するか(`Partner` は注文書・支払通知書・請求書のどれか、他は各テーブル)。期限・使用済みは見ない。
//! 2. **存在する**: ハンドラを実行し、その結果(状態コード)を `h_auth_event` に 1 件記録する(トークン自体は残さない)。
//!    存在するトークンは数えないし、IP がロック中でも通す(同じ IP の別の利用者を巻き添えにしない)。
//! 3. **存在しない・形式不正**: IP 単位で数える(15 分に 20 回、全種類で 1 つの枠)。超えたら 429。
//!    超えていなければ、`Partner` はハンドラを呼ばずに同じ形の 404 を返す(存在しない・形式不正を区別しない)。
//!    他の種類は、ハンドラをそのまま呼ぶ。存在しないトークンへのハンドラの応答は、もともとの応答(400/404/200 + valid:false など)のまま変えない。
//!    接続元 IP が解釈できないときは数えない(全員のロックを避ける)。
//!
//! ブロックの判定は、存在しないトークンの試行だけが対象。監査の記録は、ハンドラの応答を見てから書く。
//! ログ・監査にはトークンの生値を出さない(トークンの指紋=ハッシュの先頭 16 文字だけ)。
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use percent_encoding::percent_decode_str;
use serde_json::json;
use sqlx::PgPool;

use crate::infrastructure::repositories::{
    auth_repo, billing_repo, invite_repo, login_attempt_repo as repo, mobile_upload_repo, order_repo,
};
use crate::infrastructure::repositories::login_attempt_repo::Reservation;
use crate::presentation::auth_messages::{client_ip, peer_of, LOCKED_MSG};

/// 監査イベントの種別(`h_auth_event.event_type`)。detail に種類・操作・結果を入れる
pub const EVENT_TOKEN_ACCESS: &str = "token_access";

/// トークンの種類(存在の確かめ方と、監査の kind の決め方が違う)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// 注文書・支払通知書・請求書(パートナー向け)
    Partner,
    /// エンジニア招待
    Invite,
    /// 技術者ポータルの魔法リンク
    PortalLink,
    /// 経費の携帯アップロード
    MobileUpload,
}

/// トークンURLの path を解釈した結果(UUID の部分は生の文字列のまま。形式の検査は呼び出し側)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenPath {
    pub family: Family,
    pub segment: String,
    pub op: &'static str,
    /// `/api/v1/` 付き(JSON の応答にする)
    pub api: bool,
}

/// トークンURLの path を解釈する。対象外の path は None(何もしないで通す)
pub fn parse_token_path(path: &str) -> Option<TokenPath> {
    if let Some(tp) = parse_partner_path(path) {
        return Some(tp);
    }
    if let Some(rest) = path.strip_prefix("/api/v1/invite/") {
        let (segment, tail) = split_tail(rest);
        let op = match tail {
            Some("status") => "status",
            Some("accept") => "accept",
            Some("register/begin") => "register-begin",
            Some("register/complete") => "register-complete",
            _ => return None,
        };
        return Some(api_path(Family::Invite, segment, op));
    }
    if let Some(rest) = path.strip_prefix("/api/v1/auth/portal-link/") {
        let (segment, tail) = split_tail(rest);
        let op = match tail {
            None | Some("") => "view",
            Some("confirm") => "confirm",
            _ => return None,
        };
        return Some(api_path(Family::PortalLink, segment, op));
    }
    for prefix in ["/api/v1/mobile-upload/", "/api/mobile-upload/"] {
        if let Some(rest) = path.strip_prefix(prefix) {
            let (segment, tail) = split_tail(rest);
            let op = match tail {
                None | Some("") => "upload",
                Some("status") => "status",
                _ => return None,
            };
            return Some(api_path(Family::MobileUpload, segment, op));
        }
    }
    None
}

/// `/` で 1 つ目の区切りまで(トークン)と、残り(操作)に分ける。残りが無ければ None
fn split_tail(rest: &str) -> (&str, Option<&str>) {
    match rest.split_once('/') {
        Some((s, t)) => (s, Some(t)),
        None => (rest, None),
    }
}

fn api_path(family: Family, segment: &str, op: &'static str) -> TokenPath {
    TokenPath {
        family,
        segment: segment.to_string(),
        op,
        api: true,
    }
}

fn parse_partner_path(path: &str) -> Option<TokenPath> {
    let (rest, api) = match path.strip_prefix("/api/v1/token/") {
        Some(r) => (r, true),
        None => (path.strip_prefix("/token/")?, false),
    };
    let (segment, tail) = split_tail(rest);
    let op = match tail {
        None | Some("") => "view",
        Some("accept") => "accept",
        Some("pdf") => "pdf",
        Some("invoice-pdf") => "invoice-pdf",
        Some("acceptance-pdf") => "acceptance-pdf",
        Some("timesheet") => "timesheet",
        Some(_) => return None,
    };
    Some(TokenPath {
        family: Family::Partner,
        segment: segment.to_string(),
        op,
        api,
    })
}

/// 監査ログに残す、トークンの指紋(SHA-256 の先頭 16 文字)。生値は残さない
pub fn token_fingerprint(token: &uuid::Uuid) -> String {
    repo::hash_key(&token.to_string()).chars().take(16).collect()
}

/// トークンが存在するときの監査の kind を返す。存在しなければ None。期限・使用済みは見ない
async fn find_kind(pool: &PgPool, tp: &TokenPath, token: uuid::Uuid) -> anyhow::Result<Option<&'static str>> {
    match tp.family {
        Family::Partner => resolve_kind(pool, &token).await,
        Family::Invite => Ok(invite_repo::find_invitation_by_token(pool, token).await?.map(|_| "invite")),
        Family::PortalLink => Ok(auth_repo::engineer_login_token_exists(pool, token).await?.then_some("portal")),
        Family::MobileUpload => Ok(mobile_upload_repo::find_token(pool, token).await?.map(|_| "mobile")),
    }
}

/// 注文書・支払通知書・請求書のどれかに存在するか。存在すれば種類(order / notice / invoice)を返す
async fn resolve_kind(pool: &PgPool, token: &uuid::Uuid) -> anyhow::Result<Option<&'static str>> {
    if order_repo::find_purchase_order_by_uuid(pool, token).await?.is_some() {
        return Ok(Some("order"));
    }
    if order_repo::find_payment_notice_by_uuid(pool, token).await?.is_some() {
        return Ok(Some("notice"));
    }
    if billing_repo::find_invoice_by_uuid(pool, token).await?.is_some() {
        return Ok(Some("invoice"));
    }
    Ok(None)
}

/// path の {uuid} の部分を復号してから UUID として解釈する(`%2D` などのエンコード済みの有効なトークンを、有効として扱う)。
/// 復号は uuid の部分(path を `/` で分けた後の 1 区切り)だけに掛ける。復号後の `/` や `..` は UUID にならないので無効になる
pub fn decode_token(segment: &str) -> Option<uuid::Uuid> {
    let decoded = percent_decode_str(segment).decode_utf8().ok()?;
    uuid::Uuid::parse_str(&decoded).ok()
}

pub async fn token_guard(State(pool): State<PgPool>, req: Request, next: Next) -> Response {
    let Some(tp) = parse_token_path(req.uri().path()) else {
        return next.run(req).await;
    };
    let ip_text = client_ip(peer_of(&req), req.headers());
    let ip = repo::parse_ip(&ip_text);

    let token = decode_token(&tp.segment);
    let kind = match token {
        Some(t) => match find_kind(&pool, &tp, t).await {
            Ok(k) => k,
            Err(e) => {
                tracing::error!("[トークンリンク] 処理=トークンの照合 影響=リンクを開けない | {:?}", e);
                if tp.family == Family::Partner {
                    return internal_error_response();
                }
                // 他の種類は、照合できなかったときは数えずに通す(ハンドラの本来の応答に任せる)
                return next.run(req).await;
            }
        },
        None => None,
    };

    match (token, kind) {
        (Some(t), Some(kind)) => {
            // 有効なトークン: 数えない。ハンドラの結果を監査に残す(誰の・どの種類・どの操作・結果)。
            // 監査の記録は応答と切り離す(記録の遅れや失敗で、リンクの応答を遅らせない・止めない)
            let res = next.run(req).await;
            let detail = format!(
                "kind={kind} op={} status={} token={}",
                tp.op,
                res.status().as_u16(),
                token_fingerprint(&t)
            );
            let audit_pool = pool.clone();
            tokio::spawn(async move {
                auth_repo::record_token_access(&audit_pool, EVENT_TOKEN_ACCESS, &ip_text, &detail).await;
            });
            res
        }
        _ => {
            // 無効(形式不正・存在しない)。接続元 IP が取れたときだけ、IP 単位で数える
            if let Some(ip) = ip {
                match repo::reserve_invalid_token_attempt(&pool, ip).await {
                    Ok(Reservation::Allowed { .. }) => {}
                    Ok(Reservation::Locked { retry_after_secs }) => return locked_response(&tp, retry_after_secs),
                    Err(e) => {
                        tracing::error!("[トークンリンク] 処理=無効な試行の記録 影響=リンクを開けない | {:?}", e);
                        if tp.family == Family::Partner {
                            return internal_error_response();
                        }
                    }
                }
            }
            if tp.family == Family::Partner {
                return not_found_response(&tp);
            }
            // 他の種類: 存在しないトークンへのハンドラの応答(もともとの形)のまま返す
            next.run(req).await
        }
    }
}

/// 無効なトークンの応答(存在しない・形式不正で同じ形)。API は JSON、PDF 直リンクなど(非 API)は本文なし。
/// 非 API の画面 `/token/{uuid}`(view)の経路は存在しないため、ここでは扱わない。パートナー向けの応答(他の種類は使わない)
fn not_found_response(tp: &TokenPath) -> Response {
    if tp.api {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "リンクが見つかりません" }))).into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

/// IP 単位の上限を超えたときの応答(有効なトークンでは出さない)。
/// API は JSON(LOCKED_MSG と retry_after_seconds)、PDF 直リンク(非 API)は本文なしの 429。どちらも Retry-After つき
fn locked_response(tp: &TokenPath, retry_after_secs: u64) -> Response {
    let mut response = if tp.api {
        (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": LOCKED_MSG, "retry_after_seconds": retry_after_secs })),
        )
            .into_response()
    } else {
        StatusCode::TOO_MANY_REQUESTS.into_response()
    };
    if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        response.headers_mut().insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}

fn internal_error_response() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "サーバーエラーが発生しました" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_paths_are_recognised_with_their_operation() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(
            parse_token_path(&format!("/api/v1/token/{u}")),
            Some(TokenPath { family: Family::Partner, segment: u.into(), op: "view", api: true })
        );
        assert_eq!(
            parse_token_path(&format!("/api/v1/token/{u}/accept")).map(|t| t.op),
            Some("accept")
        );
        assert_eq!(
            parse_token_path(&format!("/token/{u}/acceptance-pdf")).map(|t| (t.op, t.api)),
            Some(("acceptance-pdf", false))
        );
        assert_eq!(parse_token_path(&format!("/token/{u}/pdf")).map(|t| t.op), Some("pdf"));
    }

    #[test]
    fn invite_portal_and_mobile_paths_are_recognised() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        let case = |p: &str| parse_token_path(p).map(|t| (t.family, t.op, t.segment, t.api));
        assert_eq!(case(&format!("/api/v1/invite/{u}/status")), Some((Family::Invite, "status", u.into(), true)));
        assert_eq!(case(&format!("/api/v1/invite/{u}/accept")).map(|t| t.1), Some("accept"));
        assert_eq!(case(&format!("/api/v1/invite/{u}/register/begin")).map(|t| t.1), Some("register-begin"));
        assert_eq!(case(&format!("/api/v1/invite/{u}/register/complete")).map(|t| t.1), Some("register-complete"));
        assert_eq!(case(&format!("/api/v1/auth/portal-link/{u}")), Some((Family::PortalLink, "view", u.into(), true)));
        assert_eq!(case(&format!("/api/v1/auth/portal-link/{u}/confirm")).map(|t| t.1), Some("confirm"));
        assert_eq!(case(&format!("/api/v1/mobile-upload/{u}")), Some((Family::MobileUpload, "upload", u.into(), true)));
        assert_eq!(case(&format!("/api/mobile-upload/{u}/status")).map(|t| (t.0, t.1)), Some((Family::MobileUpload, "status")));
    }

    #[test]
    fn other_paths_are_not_touched() {
        assert_eq!(parse_token_path("/api/v1/orders"), None);
        assert_eq!(parse_token_path("/api/v1/invite"), None, "招待の作成(トークンなし)は対象外");
        assert_eq!(parse_token_path("/api/v1/invite/x/unknown-op"), None);
        assert_eq!(parse_token_path("/api/v1/token/x/unknown-op"), None);
        assert_eq!(parse_token_path("/api/v1/auth/portal-link/x/unknown-op"), None);
        assert_eq!(parse_token_path("/api/v1/mobile-upload/x/unknown-op"), None);
        assert_eq!(parse_token_path("/api/v1/portal/invite"), None);
    }

    #[test]
    fn percent_encoded_uuid_is_decoded_before_the_check() {
        let u = "123e4567-e89b-12d3-a456-426614174000";
        let encoded = u.replace('-', "%2D");
        assert_eq!(decode_token(&encoded), uuid::Uuid::parse_str(u).ok());
        assert_eq!(decode_token(u), uuid::Uuid::parse_str(u).ok());
        // 復号後に区切りや相対参照になるものは、UUID にならない(無効のまま)
        assert_eq!(decode_token("..%2F..%2Fetc"), None);
        assert_eq!(decode_token("%2E%2E"), None);
        assert_eq!(decode_token("%FF%FE"), None, "UTF-8 として不正な列は無効");
    }

    #[test]
    fn fingerprint_never_contains_the_raw_token() {
        let u = uuid::Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let fp = token_fingerprint(&u);
        assert_eq!(fp.len(), 16);
        assert!(!fp.contains('-'));
        assert!(!"123e4567-e89b-12d3-a456-426614174000".contains(&fp));
    }
}
