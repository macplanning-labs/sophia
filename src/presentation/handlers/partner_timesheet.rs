/// presentation/handlers/partner_timesheet.rs — パートナー稼働報告アップロード

use axum::{
    extract::{Extension, Path, State},
    response::{IntoResponse, Redirect},
};
use sqlx::PgPool;

use crate::presentation::handlers::timesheet_upload_common;
use crate::presentation::middleware::role::AuthUser;
use crate::presentation::api_response::AppError;


/// 稼働報告一覧（パートナー向け）
pub async fn timesheet_list(
    State(_pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let _partner_id = auth_user.partner_id().unwrap_or("unknown");
    axum::response::Html("<h2>稼働報告一覧</h2>".to_string())
}

/// 稼働報告アップロード画面（GET） — SPA移行により廃止
pub async fn upload_form() -> impl IntoResponse {
    (axum::http::StatusCode::MOVED_PERMANENTLY, [(axum::http::header::LOCATION, "/")]).into_response()
}

/// 稼働報告アップロード処理（POST /portal/timesheet/{pk}）
/// パートナーがExcel / 勤務表PDFをアップロード → 解析 → DB保存
/// 初期ステータスは UPLOADED（パートナー提出）。
pub async fn upload_submit(
    State(pool): State<PgPool>,
    Path(pk): Path<i64>,
    Extension(auth_user): Extension<AuthUser>,
    mut multipart: axum::extract::Multipart,
) -> impl IntoResponse {
    let partner_id = auth_user.partner_id().unwrap_or("unknown").to_string();

    let (bytes, original_filename) =
        match timesheet_upload_common::extract_upload_file(&mut multipart).await {
            Ok(v) => v,
            Err(e) => return axum::Json(e.to_json()).into_response(),
        };

    let result = match timesheet_upload_common::parse_upload(&bytes, &original_filename) {
        Ok(r) => r,
        Err(e) => return axum::Json(e.to_json()).into_response(),
    };

    let target_month = timesheet_upload_common::normalize_target_month(&result);

    let contract_id = match timesheet_upload_common::resolve_partner_contract_id(
        &pool,
        &partner_id,
        &result.worker_name,
        target_month,
        pk,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => return axum::Json(e.to_json()).into_response(),
    };

    if let Err(e) = timesheet_upload_common::ensure_received_order(
        &pool,
        contract_id,
        target_month,
        &result.worker_name,
    )
    .await
    {
        return axum::Json(e.to_json()).into_response();
    }

    let mut result = result;
    if let Err(e) = timesheet_upload_common::apply_settlement_hours_for_contract(
        &pool,
        contract_id,
        &mut result,
    )
    .await
    {
        return axum::Json(e.to_json()).into_response();
    }

    match timesheet_upload_common::upsert_parsed_timesheet(
        &pool,
        contract_id,
        target_month,
        &result,
        &original_filename,
        "UPLOADED",
    )
    .await
    {
        Ok(()) => {
            tracing::info!(
                "パートナー稼働報告アップロード: {} → {}h / {}日",
                original_filename,
                result.total_hours,
                result.work_days
            );
            // 既存クライアント互換のため confirm 成功時も preview を返す
            axum::Json(serde_json::json!({
                "status": "ok",
                "preview": timesheet_upload_common::build_preview_json(&result, &original_filename),
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!("パートナー稼働報告DB保存エラー: {:?}", e);
            axum::Json(e.to_json()).into_response()
        }
    }
}

/// 稼働報告 提出（ステータス変更）
pub async fn submit_timesheet(
    State(_pool): State<PgPool>,
    Path(_pk): Path<i64>,
    Extension(_auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    Redirect::to("/portal")
}

// ── SPA用 JSON API ──

/// GET /api/portal/timesheets
pub async fn api_timesheet_list(
    State(pool): State<PgPool>,
    Extension(auth_user): Extension<AuthUser>,
) -> impl IntoResponse {
    let partner_id = auth_user.partner_id().unwrap_or("unknown");
    let timesheets: Vec<serde_json::Value> = crate::infrastructure::repositories::timesheet_repo::list_timesheets_for_partner(&pool, partner_id)
        .await
        .unwrap_or_default()
        .into_iter()
    .map(|(id, month, status, engineer, hours)| serde_json::json!({
        "id": id, "target_month": month, "status": status, "engineer_name": engineer, "total_hours": hours,
    })).collect();
    axum::Json(serde_json::json!({ "timesheets": timesheets }))
}

/// POST /api/portal/timesheets/{pk}/submit
pub async fn api_submit_timesheet(
    State(pool): State<PgPool>,
    Path(pk): Path<i64>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<impl IntoResponse, AppError> {
    let partner_id = auth_user.partner_id().unwrap_or("unknown");
    let rows = crate::infrastructure::repositories::timesheet_repo::submit_timesheet_for_partner(&pool, pk, partner_id).await?;
    Ok(axum::Json(serde_json::json!({ "success": rows > 0 })))
}
