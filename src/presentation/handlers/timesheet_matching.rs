/// presentation/handlers/timesheet_matching.rs — 勤務表（メール添付）と受注の結び付け
///
/// - GET  /api/v1/timesheet-matching?month=YYYY-MM              — 左: 受注 / 右: 届いた勤務表と提案
/// - GET  /api/v1/timesheet-attachments/{id}/file                — 勤務表の原本
/// - GET  /api/v1/timesheet-attachments/{id}/preview?order_id=   — 取り込み確認（DBは更新しない）
/// - POST /api/v1/timesheet-attachments/{id}/import              — 取り込み + 承認

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use chrono::NaiveDate;
use sqlx::PgPool;

use crate::infrastructure::mail_pipeline::timesheet_matching;

fn bad_request(msg: &str) -> Response {
    (
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({ "success": false, "error": msg })),
    )
        .into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct BoardQuery {
    /// 対象月（YYYY-MM）。省略時は当月
    pub month: Option<String>,
}

/// GET /api/v1/timesheet-matching
pub async fn api_board(State(pool): State<PgPool>, Query(q): Query<BoardQuery>) -> Response {
    let month = match q.month.as_deref() {
        Some(m) => match NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => return bad_request("対象月は YYYY-MM で指定してください"),
        },
        None => {
            use chrono::Datelike;
            let today = chrono::Local::now().date_naive();
            today.with_day(1).unwrap_or(today)
        }
    };
    match timesheet_matching::board(&pool, month).await {
        Ok(v) => axum::Json(serde_json::json!({ "success": true, "board": v })).into_response(),
        Err(msg) => {
            tracing::error!("api_board: {msg}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "success": false, "error": msg })),
            )
                .into_response()
        }
    }
}

/// GET /api/v1/timesheet-attachments/{id}/file
pub async fn api_attachment_file(State(pool): State<PgPool>, Path(id): Path<i64>) -> Response {
    match timesheet_matching::attachment_file(&pool, id).await {
        Ok((filename, bytes)) => {
            let content_type = if filename.to_lowercase().ends_with(".pdf") {
                "application/pdf"
            } else {
                "application/octet-stream"
            };
            crate::presentation::http_util::build_response(
                axum::http::Response::builder()
                    .status(axum::http::StatusCode::OK)
                    .header(axum::http::header::CONTENT_TYPE, content_type)
                    .header("X-Content-Type-Options", "nosniff")
                    .header(axum::http::header::CACHE_CONTROL, "private, no-store"),
                axum::body::Body::from(bytes),
            )
        }
        Err(msg) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "success": false, "error": msg })),
        )
            .into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct PreviewQuery {
    pub order_id: i64,
}

/// GET /api/v1/timesheet-attachments/{id}/preview?order_id=
pub async fn api_attachment_preview(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Query(q): Query<PreviewQuery>,
) -> Response {
    match timesheet_matching::preview_attachment(&pool, id, q.order_id).await {
        Ok(v) => axum::Json(serde_json::json!({ "success": true, "preview": v })).into_response(),
        Err(msg) => {
            tracing::warn!("api_attachment_preview: attachment_id={id} {msg}");
            bad_request(&msg)
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ImportBody {
    pub order_id: i64,
}

/// POST /api/v1/timesheet-attachments/{id}/import — 勤務表を稼働報告に取り込み、承認する
///
/// 画面で氏名・時間・結び付け先の受注を確認した上で押される。承認前の同月の稼働報告があれば置き換える。
pub async fn api_attachment_import(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<ImportBody>,
) -> Response {
    let timesheet_id = match timesheet_matching::import_attachment(&pool, id, body.order_id).await {
        Ok(tid) => tid,
        Err(msg) => {
            tracing::warn!("api_attachment_import: attachment_id={id} {msg}");
            return bad_request(&msg);
        }
    };
    // 承認（承認通知メールなどの副作用は、稼働報告の承認APIと同じ処理を使う）
    let approve_res = super::timesheets::approve(State(pool), Path(timesheet_id)).await.into_response();
    if !approve_res.status().is_success() {
        return (
            axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({
                "success": false,
                "error": "稼働報告は取り込みましたが、承認に失敗しました。稼働報告画面から承認してください",
                "timesheet_id": timesheet_id
            })),
        )
            .into_response();
    }
    axum::Json(serde_json::json!({ "success": true, "timesheet_id": timesheet_id })).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct RejectBody {
    #[serde(default)]
    pub reason: String,
}

/// POST /api/v1/timesheet-attachments/{id}/reject — 勤務表を差し戻す（理由を記録。メールは送らない）
pub async fn api_attachment_reject(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<RejectBody>,
) -> Response {
    match timesheet_matching::reject_attachment(&pool, id, &body.reason).await {
        Ok(()) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(msg) => {
            tracing::warn!("api_attachment_reject: attachment_id={id} {msg}");
            bad_request(&msg)
        }
    }
}

/// POST /api/v1/timesheet-attachments/{id}/unreject — 差し戻しを取り消す
pub async fn api_attachment_unreject(State(pool): State<PgPool>, Path(id): Path<i64>) -> Response {
    match timesheet_matching::unreject_attachment(&pool, id).await {
        Ok(()) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(msg) => {
            tracing::warn!("api_attachment_unreject: attachment_id={id} {msg}");
            bad_request(&msg)
        }
    }
}
