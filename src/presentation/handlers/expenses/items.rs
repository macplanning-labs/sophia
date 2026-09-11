/// expenses/items.rs — 経費申請の明細（行）CRUD + 領収書画像取得/PCアップロード

use axum::{
    extract::{Extension, Multipart, Path, State},
    response::{IntoResponse, Response},
    Json,
};
use axum::http::{header, StatusCode};
use axum::body::Body;
use sqlx::PgPool;

use crate::domain::models::expense::ExpenseRequestItemForm;
use crate::infrastructure::repositories::{expense_repo, mobile_upload_repo};
use crate::presentation::api_response::AppError;
use crate::presentation::middleware::role::AuthUser;

/// マジックバイト優先で MIME を判定（クリップボード貼り付け時に content-type が空/不正確なことがある）
pub(crate) fn resolve_receipt_mime(bytes: &[u8], reported: &str) -> Option<&'static str> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some("image/jpeg");
    }
    if bytes.len() >= 8 && bytes[0..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some("image/png");
    }
    // PDF: "%PDF"
    if bytes.len() >= 4 && bytes[0..4] == [b'%', b'P', b'D', b'F'] {
        return Some("application/pdf");
    }
    let reported = reported.to_ascii_lowercase();
    match reported.as_str() {
        "image/jpeg" | "image/jpg" => Some("image/jpeg"),
        "image/png" => Some("image/png"),
        "application/pdf" => Some("application/pdf"),
        _ => None,
    }
}

/// POST /api/expenses/{id}/items — 明細を追加する
pub async fn api_add_item(
    State(pool): State<PgPool>,
    Path(expense_request_id): Path<i64>,
    Json(form): Json<ExpenseRequestItemForm>,
) -> Result<impl IntoResponse, AppError> {
    let item_id = expense_repo::add_item(&pool, expense_request_id, &form).await?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "success": true, "id": item_id }))))
}

/// PUT /api/expenses/items/{item_id} — 明細を更新する
pub async fn api_update_item(
    State(pool): State<PgPool>,
    Path(item_id): Path<i64>,
    Json(form): Json<ExpenseRequestItemForm>,
) -> Result<impl IntoResponse, AppError> {
    let rows = expense_repo::update_item(&pool, item_id, &form).await?;
    Ok(Json(serde_json::json!({ "success": rows > 0 })))
}

/// DELETE /api/expenses/items/{item_id} — 明細を削除する
pub async fn api_delete_item(
    State(pool): State<PgPool>,
    Path(item_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let rows = expense_repo::delete_item(&pool, item_id).await?;
    Ok(Json(serde_json::json!({ "success": rows > 0 })))
}

/// GET /api/expenses/items/{item_id}/receipt — 領収書画像を取得する
pub async fn api_item_receipt(
    State(pool): State<PgPool>,
    Path(item_id): Path<i64>,
) -> Result<Response, AppError> {
    let image = expense_repo::find_receipt_image(&pool, item_id).await?;

    match image {
        Some((bytes, mime)) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header(header::CACHE_CONTROL, "private, max-age=3600")
            .body(Body::from(bytes))
            .expect("Response builder should not fail")),
        None => Ok((StatusCode::NOT_FOUND, "領収書画像が見つかりません").into_response()),
    }
}

/// POST /api/expenses/items/{item_id}/receipt — PC画面から領収書画像をアップロード（貼り付け/ファイル選択）
pub async fn api_upload_item_receipt(
    State(pool): State<PgPool>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(item_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let item = expense_repo::find_item_by_id(&pool, item_id).await?;
    if item.is_none() {
        return Ok((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"success": false, "error": "明細が見つかりません"})),
        )
            .into_response());
    }

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut content_type = String::new();

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                return Ok((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "success": false,
                        "error": format!("アップロードデータの解析に失敗しました: {e}")
                    })),
                )
                    .into_response());
            }
        };

        if field.name() == Some("file") {
            content_type = field.content_type().unwrap_or("").to_string();
            match field.bytes().await {
                Ok(bytes) => file_bytes = Some(bytes.to_vec()),
                Err(e) => {
                    return Ok((
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({
                            "success": false,
                            "error": format!("ファイルの読み込みに失敗しました: {e}")
                        })),
                    )
                        .into_response());
                }
            }
        }
    }

    let Some(bytes) = file_bytes else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "ファイルが選択されていません"})),
        )
            .into_response());
    };

    if bytes.len() > mobile_upload_repo::MAX_UPLOAD_BYTES {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "ファイルサイズが大きすぎます（上限10MB）"})),
        )
            .into_response());
    }

    let Some(mime) = resolve_receipt_mime(&bytes, &content_type) else {
        return Ok((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"success": false, "error": "JPEG・PNGまたはPDF形式のみアップロードできます"})),
        )
            .into_response());
    };

    expense_repo::save_receipt_image(&pool, item_id, &bytes, mime).await?;
    tracing::info!("PCから領収書を受信: item_id={}, mime={}, bytes={}", item_id, mime, bytes.len());

    Ok(Json(serde_json::json!({ "success": true })).into_response())
}
