//! presentation/middleware/login_attempt.rs — パスワードログインの試行制限(DB 保存。DEMO-000233 S2-D / 項目6-1)
//!
//! `/api/auth/login`・`/api/v1/auth/login` の前に置く。順序は次のとおり(判定の前に記録する):
//! 1. 本文から「メール」を取り出す(取り出せない本文は、そのままハンドラへ渡す。ハンドラが拒否する)
//! 2. `reserve_password_attempt` で 1 件記録し、アカウント単位(15 分に 30 回)と IP + アカウント(5 回)を数える
//! 3. 上限を超えていれば、ハンドラを呼ばずに 429 を返す(正しいパスワードでも断る。その試行は記録しない)
//! 4. ハンドラの応答で後始末: 成功は、同じ IP の失敗を無効化 / 失敗(401・422)はそのまま数える / それ以外は記録を消す
//!
//! 応答の形(429 の `account_locked`、`Retry-After`)は、以前の auth-core の試行ロックと同じにしてある。
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use auth_core::domain::attempt_lock::{AttemptOutcome, AttemptOutcomeKind, DefaultAttemptOutcome};
use serde_json::json;
use sqlx::PgPool;

use crate::infrastructure::repositories::login_attempt_repo::{self as repo, Reservation};
use crate::presentation::auth_messages::{client_ip, peer_of};
use crate::presentation::handlers::auth::LoginForm;

/// 本文の読み取りの上限(ログインフォームは数百バイトで足りる)
const MAX_BODY_BYTES: usize = 32 * 1024;

pub async fn guard(pool: PgPool, req: Request, next: Next) -> Response {
    let ip_text = client_ip(peer_of(&req), req.headers());
    let ip = repo::parse_ip(&ip_text);

    let (parts, body) = req.into_parts();
    let bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(json!({ "error": "payload_too_large", "limit_bytes": MAX_BODY_BYTES })),
            )
                .into_response()
        }
    };

    let Some(account) = serde_json::from_slice::<LoginForm>(&bytes)
        .ok()
        .map(|form| repo::normalize_account(&form.email))
    else {
        // 解釈できない本文はハンドラに任せる(ハンドラの Json 抽出が拒否する)
        return next.run(Request::from_parts(parts, Body::from(bytes))).await;
    };

    let id = match repo::reserve_password_attempt(&pool, &account, ip).await {
        Ok(Reservation::Allowed { id }) => id,
        Ok(Reservation::Locked { retry_after_secs }) => return locked_response(retry_after_secs),
        Err(e) => {
            tracing::error!("ログイン試行の記録に失敗: {:?}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "internal_error", "detail": "試行回数の確認に失敗しました" })),
            )
                .into_response();
        }
    };

    let response = next.run(Request::from_parts(parts, Body::from(bytes))).await;

    match DefaultAttemptOutcome.classify(response.status()) {
        Some(AttemptOutcomeKind::Failure) => {}
        Some(AttemptOutcomeKind::Success) => {
            let key_hash = repo::hash_key(&account);
            if let Err(e) = repo::invalidate_failures(&pool, repo::KIND_PASSWORD, &key_hash, ip).await {
                tracing::error!("ログイン成功後の試行の無効化に失敗: {:?}", e);
            }
        }
        None => {
            if let Err(e) = repo::delete_attempt(&pool, id).await {
                tracing::error!("ログイン試行の記録の削除に失敗: {:?}", e);
            }
        }
    }
    response
}

fn locked_response(retry_after_secs: u64) -> Response {
    let mut response = (
        StatusCode::TOO_MANY_REQUESTS,
        Json(json!({ "error": "account_locked", "retry_after_seconds": retry_after_secs })),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::RETRY_AFTER, value);
    }
    response
}
