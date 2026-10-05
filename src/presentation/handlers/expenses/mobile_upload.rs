/// expenses/mobile_upload.rs — スマホカメラ連携アップロード機能
///
/// PCの経費申請画面（明細行）から発行したワンタイムトークンをQRコード化し、
/// スマホがそれを読み取って領収書写真をアップロードする。
///
/// - トークンは10分で失効し、1回アップロードに成功したら即座に使い切りになる
///   （`mobile_upload_repo::mark_used`のUPDATE ... WHERE used_at IS NULLがCAS的に
///   1回しか成功しないため、同時に2回リクエストが来ても二重書き込みは起きない）
/// - `POST /api/mobile-upload/{token}`・`GET /api/mobile-upload/{token}/status`は
///   スマホ側にPCのセッションCookieがないため、`middleware/auth.rs`で認証不要パスに
///   登録している（トークン自体がUUID+期限+使い捨てで自己完結した認証情報のため）

use axum::{
    extract::{Extension, Multipart, Path, State},
    response::IntoResponse,
    Json,
};
use axum::http::StatusCode;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::repositories::{expense_repo, mobile_upload_repo};
use crate::presentation::api_response::AppError;
use crate::presentation::middleware::role::AuthUser;

use super::items::resolve_receipt_mime;

/// POST /api/expenses/items/{item_id}/mobile-upload/token — トークン発行（PCセッション認証必須。
/// 認証自体はルーティング側のミドルウェアが担うため、ここでは`AuthUser`を要求することで
/// 未認証リクエストを弾く以上の用途はない）
pub async fn api_issue_mobile_token(
    State(pool): State<PgPool>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(item_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // 明細の存在確認＋申請中のみトークン発行（PENDING/DRAFT）
    let item = expense_repo::find_item_by_id(&pool, item_id).await?;
    if item.is_none() {
        return Ok((StatusCode::NOT_FOUND,
            Json(serde_json::json!({"success": false, "error": "明細が見つかりません"}))).into_response());
    }
    if let Some((_header_id, status)) = expense_repo::find_item_owner_status(&pool, item_id).await? {
        if !crate::domain::services::document_workflow::can_edit_content(&status) {
            return Ok((StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"success": false, "error": "申請中の経費のみ領収書をアップロードできます"}))).into_response());
        }
    }

    let token = mobile_upload_repo::issue_token(&pool, item_id).await?;

    let base_url = std::env::var("BASE_URL")
        .or_else(|_| std::env::var("FRONTEND_URL"))
        .unwrap_or_else(|_| "http://localhost:8110".to_string());
    let upload_url = format!("{}/upload/mobile/{}", base_url, token.token);

    Ok(Json(serde_json::json!({
        "success": true,
        "token": token.token,
        "upload_url": upload_url,
        "expires_at": token.expires_at,
    })).into_response())
}

/// POST /api/mobile-upload/{token} — スマホからの画像アップロード（認証不要・トークン自体が認証情報）
pub async fn api_mobile_upload(
    State(pool): State<PgPool>,
    Path(token): Path<Uuid>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let record = match mobile_upload_repo::find_token(&pool, token).await {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({"success": false, "error": "無効なリンクです"}))).into_response(),
        Err(e) => {
            tracing::error!("mobile-upload: トークン取得失敗: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"success": false, "error": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    if !record.is_valid(Utc::now()) {
        return (StatusCode::GONE,
            Json(serde_json::json!({"success": false, "error": "リンクの有効期限が切れているか、既に使用済みです"}))).into_response();
    }

    // multipartからファイルフィールド("file")を読み取る
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type = String::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return (StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"success": false, "error": format!("アップロードデータの解析に失敗しました: {e}")}))).into_response();
            }
        };

        if field.name() == Some("file") {
            content_type = field.content_type().unwrap_or("").to_string();
            match field.bytes().await {
                Ok(bytes) => file_bytes = Some(bytes.to_vec()),
                Err(e) => {
                    return (StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({"success": false, "error": format!("ファイルの読み込みに失敗しました: {e}")}))).into_response();
                }
            }
        }
    }

    let Some(bytes) = file_bytes else {
        return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "ファイルが選択されていません"}))).into_response();
    };

    if bytes.len() > mobile_upload_repo::MAX_UPLOAD_BYTES {
        return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "ファイルサイズが大きすぎます（上限10MB）"}))).into_response();
    }

    let Some(mime) = resolve_receipt_mime(&bytes, &content_type) else {
        return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "JPEG・PNGまたはPDF形式のみアップロードできます"}))).into_response();
    };

    // トークンを使い切りにする（同時多重送信があってもここのUPDATEはWHERE used_at IS NULLで
    // 1回しか成功しないため、後続の保存だけ複数回走ることはあっても最終的な画像は最後勝ちで無害）
    let marked = match mobile_upload_repo::mark_used(&pool, token).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("mobile-upload: トークン使用済み化失敗: {:?}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"success": false, "error": "サーバーエラーが発生しました"}))).into_response();
        }
    };

    if marked == 0 {
        // 既に他のリクエストが使用済みにしていた（同時送信の競合）
        return (StatusCode::GONE,
            Json(serde_json::json!({"success": false, "error": "このリンクは既に使用されています"}))).into_response();
    }

    if let Err(e) = expense_repo::save_receipt_image(&pool, record.expense_request_item_id, &bytes, mime).await {
        tracing::error!("mobile-upload: 領収書保存失敗: {:?}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"success": false, "error": "ファイルの保存に失敗しました"}))).into_response();
    }

    tracing::info!("スマホから領収書画像を受信: item_id={}", record.expense_request_item_id);
    Json(serde_json::json!({ "success": true })).into_response()
}

/// GET /api/mobile-upload/{token}/status — PC側のアップロード完了ポーリング（認証不要）
pub async fn api_mobile_upload_status(
    State(pool): State<PgPool>,
    Path(token): Path<Uuid>,
) -> impl IntoResponse {
    match mobile_upload_repo::find_token(&pool, token).await {
        Ok(Some(record)) => {
            let uploaded = record.used_at.is_some();
            let expired = record.expires_at <= Utc::now();
            let receipt_mime = if uploaded {
                expense_repo::find_item_by_id(&pool, record.expense_request_item_id)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|i| i.receipt_mime)
            } else {
                None
            };
            Json(serde_json::json!({
                "uploaded": uploaded,
                "expired": expired && !uploaded,
                "expense_request_item_id": record.expense_request_item_id,
                "receipt_mime": receipt_mime,
            })).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "無効なトークンです"}))).into_response(),
        Err(e) => {
            tracing::error!("mobile-upload status: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "サーバーエラーが発生しました"}))).into_response()
        }
    }
}
