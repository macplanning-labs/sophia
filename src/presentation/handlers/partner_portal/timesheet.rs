use axum::{
    extract::{Extension, Multipart, State},
    response::IntoResponse,
};
use sqlx::PgPool;
use crate::presentation::handlers::timesheet_upload_common;
use crate::presentation::middleware::role::AuthUser;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// 稼働報告（解析 + 確定登録の2ステップ）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// POST /api/portal/timesheets/upload — 稼働報告書の解析のみ（DBに保存しない）
///
/// Excel / 勤務表PDFを受け取り、共通パーサで解析し、プレビューJSONを返す。
pub async fn api_timesheet_upload(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let partner_id = auth_user.partner_id().unwrap_or("unknown");

    let (file_bytes, original_filename) =
        match timesheet_upload_common::extract_upload_file(&mut multipart).await {
            Ok(v) => v,
            Err(e) => return axum::Json(e.to_json()),
        };

    let result = match timesheet_upload_common::parse_upload(&file_bytes, &original_filename) {
        Ok(r) => r,
        Err(e) => return axum::Json(e.to_json()),
    };

    let result = match timesheet_upload_common::prepare_partner_preview_result(
        &pool,
        partner_id,
        result,
        0,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return axum::Json(e.to_json()),
    };

    axum::Json(timesheet_upload_common::ok_preview_response(
        &result,
        &original_filename,
    ))
}

/// POST /api/portal/timesheets/confirm — 稼働報告の確定登録
///
/// パートナーがプレビューを確認後、再送されたファイルを解析→DB保存する。
/// 既存の upload_submit と同様のDB保存ロジックを使用（ステータス UPLOADED）。
pub async fn api_timesheet_confirm(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
    multipart: Multipart,
) -> impl IntoResponse {
    crate::presentation::handlers::partner_timesheet::upload_submit(
        State(pool),
        axum::extract::Path(0i64),
        Extension(auth_user),
        multipart,
    )
    .await
}
