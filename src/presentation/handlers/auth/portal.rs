use crate::presentation::cookie_util;
use crate::presentation::login_guard::{self, GuardStatus};
use crate::infrastructure::repositories::auth_repo;
use crate::infrastructure::repositories::invite_repo;
use axum::{
    extract::State,
    response::IntoResponse,
};
use sqlx::PgPool;

// ── パートナーポータル認証（マジックリンク）──

/// POST /api/auth/portal-login — マジックリンク送信
///
/// エンジニアのメールアドレスを受け取り、ログインURLをメール送信する。
/// パスワード不要。リンクの有効期限は24時間。
pub async fn api_portal_login(
    State(pool): State<PgPool>,
    headers: axum::http::HeaderMap,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let email = payload["email"].as_str().unwrap_or_default().trim();
    if email.is_empty() || !email.contains('@') {
        return axum::Json(serde_json::json!({
            "success": false, "error": "メールアドレスを入力してください"
        })).into_response();
    }

    let ip = login_guard::client_ip(&headers);
    let guard_key = login_guard::portal_key(&ip, email);
    if let GuardStatus::Locked { retry_after_secs } = login_guard::check(&guard_key) {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
            axum::Json(serde_json::json!({
                "success": false,
                "error": login_guard::LOCKED_MSG,
                "retry_after_secs": retry_after_secs
            })),
        ).into_response();
    }
    // Count each portal-login attempt (anti email-bomb / enumeration probe)
    if let GuardStatus::Locked { retry_after_secs } = login_guard::record_failure(&guard_key) {
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            [(axum::http::header::RETRY_AFTER, retry_after_secs.to_string())],
            axum::Json(serde_json::json!({
                "success": false,
                "error": login_guard::LOCKED_MSG,
                "retry_after_secs": retry_after_secs
            })),
        ).into_response();
    }

    // m_engineer からメールアドレスで検索
    let engineer = auth_repo::find_engineer_by_email(&pool, email).await.ok().flatten();

    let Some((engineer_id, engineer_name)) = engineer else {
        // セキュリティ: メールが存在しなくても同じレスポンスを返す
        tracing::info!("ポータルログイン: メールアドレス不一致 ({})", email);
        return axum::Json(serde_json::json!({
            "success": true,
            "message": "ログインリンクを送信しました。メールを確認してください。"
        })).into_response();
    };

    // ログイントークン生成（24時間有効）
    let token = uuid::Uuid::new_v4();
    let expires_at = chrono::Utc::now() + chrono::Duration::hours(24);

    if let Err(e) = auth_repo::insert_engineer_login_token(&pool, engineer_id, token, expires_at).await {
        tracing::error!("ログイントークン生成エラー: {}", e);
        return axum::Json(serde_json::json!({
            "success": false, "error": "システムエラーが発生しました"
        })).into_response();
    }

    // ログインURL組み立て
    let base_url = std::env::var("BASE_URL")
        .or_else(|_| std::env::var("FRONTEND_URL"))
        .unwrap_or_else(|_| "http://localhost:8110".to_string());
    let login_url = format!("{}/portal/auth/{}", base_url, token);

    // メール送信
    let email_service = crate::domain::services::email_service::EmailService::new(pool.clone());
    let mut context = std::collections::HashMap::new();
    context.insert("engineer_name".to_string(), engineer_name.clone());
    context.insert("login_url".to_string(), login_url.clone());

    match email_service.send_by_template("ENGINEER_LOGIN_LINK", email, None, &context).await {
        Ok(_) => tracing::info!("ポータルログインリンク送信: {} ({})", engineer_name, email),
        Err(e) => tracing::error!("ポータルログインリンク送信失敗: {} - {}", email, e),
    }

    axum::Json(serde_json::json!({
        "success": true,
        "message": "ログインリンクを送信しました。メールを確認してください。"
    })).into_response()
}

/// GET /api/v1/auth/portal-link/{token} — マジックリンク有効性（トークン非消費）
pub async fn api_portal_link_status(
    State(pool): State<PgPool>,
    axum::extract::Path(token_str): axum::extract::Path<String>,
) -> impl IntoResponse {
    let token = match uuid::Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "valid": false, "error": "無効なリンクです" })),
            )
                .into_response();
        }
    };
    let exists = auth_repo::engineer_login_token_valid(&pool, token).await.unwrap_or(false);
    if !exists {
        return axum::Json(serde_json::json!({
            "valid": false,
            "error": "リンクの有効期限が切れています"
        }))
        .into_response();
    }
    axum::Json(serde_json::json!({ "valid": true })).into_response()
}

/// POST /api/v1/auth/portal-link/{token}/confirm — トークン消費＋セッション作成
pub async fn api_portal_link_confirm(
    State(pool): State<PgPool>,
    axum::extract::Path(token_str): axum::extract::Path<String>,
) -> impl IntoResponse {
    let token = match uuid::Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "success": false, "error": "無効なリンクです" })),
            )
                .into_response();
        }
    };

    let login_token = auth_repo::find_valid_engineer_login_token(&pool, token).await.ok().flatten();
    let Some((token_id, engineer_id)) = login_token else {
        return (
            axum::http::StatusCode::GONE,
            axum::Json(serde_json::json!({
                "success": false,
                "error": "リンクの有効期限が切れています"
            })),
        )
            .into_response();
    };

    if let Err(e) = auth_repo::mark_engineer_login_token_used(&pool, token_id).await {
        tracing::error!("ログイントークンの使用済み更新に失敗: token_id={} {:?}", token_id, e);
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = invite_repo::insert_engineer_session(&pool, &session_id, engineer_id).await {
        tracing::error!("エンジニアセッション作成に失敗: engineer_id={} {:?}", engineer_id, e);
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({
                "success": false,
                "error": "ログイン処理に失敗しました"
            })),
        )
            .into_response();
    }

    tracing::info!("エンジニアセッション作成: engineer_id={}", engineer_id);
    let cookie = cookie_util::sophia_session_header(&session_id, 30 * 24 * 60 * 60);
    (
        axum::http::StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie)],
        axum::Json(serde_json::json!({
            "success": true,
            "redirect": "/portal"
        })),
    )
        .into_response()
}

/// GET /portal/auth/{token} — マジックリンク確認ページ表示（HTML レガシー・廃止予定）
///
/// トークンの有効性を検証し、「ログインする」ボタン付きの確認ページを表示。
/// GETではトークンを消費しない（メールクライアントのプリフェッチ対策）。
pub async fn portal_auth_verify(
    State(pool): State<PgPool>,
    axum::extract::Path(token_str): axum::extract::Path<String>,
) -> impl IntoResponse {
    let token = match uuid::Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => return axum::response::Html(
            "<h2>無効なリンクです</h2><p><a href='/portal/login'>ログインページへ</a></p>"
            .to_string()
        ).into_response(),
    };

    // トークン有効性チェック（消費はしない）
    let exists = auth_repo::engineer_login_token_valid(&pool, token).await.unwrap_or(false);

    if !exists {
        return axum::response::Html(
            "<h2>リンクの有効期限が切れています</h2><p><a href='/portal/login'>新しいリンクを取得する</a></p>"
            .to_string()
        ).into_response();
    }

    // 確認ページを表示（ボタンクリックでPOST）
    axum::response::Html(format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Sophia ログイン</title>
<style>
  body {{ font-family: -apple-system, sans-serif; background: #0a0a0a; color: #e5e5e5; display: flex; justify-content: center; align-items: center; min-height: 100vh; margin: 0; }}
  .card {{ background: #1a1a1a; border: 1px solid #333; border-radius: 12px; padding: 40px; max-width: 400px; text-align: center; }}
  h1 {{ font-size: 28px; background: linear-gradient(to right, #34d399, #2dd4bf); -webkit-background-clip: text; -webkit-text-fill-color: transparent; margin-bottom: 8px; }}
  p {{ color: #a3a3a3; font-size: 14px; }}
  button {{ width: 100%; padding: 12px; background: linear-gradient(to right, #10b981, #14b8a6); color: white; border: none; border-radius: 8px; font-size: 16px; font-weight: 600; cursor: pointer; margin-top: 24px; }}
  button:hover {{ opacity: 0.9; }}
</style>
</head><body>
<div class="card">
  <h1>Sophia</h1>
  <p>パートナーポータル</p>
  <form method="POST" action="/portal/auth/{}">
    <button type="submit">ログインする</button>
  </form>
</div>
</body></html>"#,
        token_str
    )).into_response()
}

/// POST /portal/auth/{token} — マジックリンクでトークン消費＋セッション作成
///
/// ユーザーが確認ページの「ログインする」ボタンをクリックした時のみ実行。
pub async fn portal_auth_confirm(
    State(pool): State<PgPool>,
    axum::extract::Path(token_str): axum::extract::Path<String>,
) -> impl IntoResponse {
    let token = match uuid::Uuid::parse_str(&token_str) {
        Ok(t) => t,
        Err(_) => return axum::response::Html(
            "<h2>無効なリンクです</h2><p><a href='/portal/login'>ログインページへ</a></p>"
            .to_string()
        ).into_response(),
    };

    // トークン検証
    let login_token = auth_repo::find_valid_engineer_login_token(&pool, token).await.ok().flatten();

    let Some((token_id, engineer_id)) = login_token else {
        return axum::response::Html(
            "<h2>リンクの有効期限が切れています</h2><p><a href='/portal/login'>新しいリンクを取得する</a></p>"
            .to_string()
        ).into_response();
    };

    // トークンを使用済みに（失敗してもログインは継続する。再利用可能なトークンが残るだけなので致命的ではない）
    if let Err(e) = auth_repo::mark_engineer_login_token_used(&pool, token_id).await {
        tracing::error!("ログイントークンの使用済み更新に失敗: token_id={} {:?}", token_id, e);
    }

    // セッション作成（30日有効）。失敗した場合、Cookieだけ発行すると
    // 「ログインしたのに認証されない」壊れた状態になるためCookieを発行せずエラー画面を返す
    let session_id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = invite_repo::insert_engineer_session(&pool, &session_id, engineer_id).await {
        tracing::error!("エンジニアセッション作成に失敗: engineer_id={} {:?}", engineer_id, e);
        return axum::response::Html(
            "<h2>ログイン処理に失敗しました</h2><p>時間をおいて再度お試しください。</p>".to_string()
        ).into_response();
    }

    tracing::info!("エンジニアセッション作成: engineer_id={}", engineer_id);

    // Cookie設定 + リダイレクト（BASE_URLでフルURL指定、ポート番号保持）
    let base_url = std::env::var("BASE_URL")
        .or_else(|_| std::env::var("FRONTEND_URL"))
        .unwrap_or_else(|_| "http://localhost:8110".to_string());
    let redirect_url = format!("{}/portal", base_url);
    let cookie = cookie_util::sophia_session_header(&session_id, 30 * 24 * 60 * 60);
    (
        axum::http::StatusCode::SEE_OTHER,
        [
            (axum::http::header::SET_COOKIE, cookie),
            (axum::http::header::LOCATION, redirect_url),
        ],
    ).into_response()
}
