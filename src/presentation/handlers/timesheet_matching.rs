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
pub async fn api_board(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Query(q): Query<BoardQuery>,
) -> Response {
    // 担当スタンプを持たない操作(一覧・選択肢・新規作成)。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetView) {
        return crate::presentation::http_util::forbidden();
    }
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
    match timesheet_matching::board(&pool, month, &auth_user.list_filter(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::OrderReceivedView)).await {
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
pub async fn api_attachment_file(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> Response {
    // 受信メールの添付は担当スタンプを持たない(メールの領域)。権限を持つ人だけが取れる。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetView) {
        return crate::presentation::http_util::forbidden();
    }
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
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
    Query(q): Query<PreviewQuery>,
) -> Response {
    // 原本の取得(api_attachment_file)と同じ。添付は担当スタンプを持たず、権限を持つ人だけが見られる(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetView) {
        return crate::presentation::http_util::forbidden();
    }
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
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<ImportBody>,
) -> Response {
    // 入口の権限表に頼らず、ハンドラ自身で判定する(DEMO-000254)。添付は担当スタンプを持たないので、権限の有無を見る。
    // 取り込み先の受注(担当スタンプを持つ)も、編集の権限と担当範囲の中であること。取り込み(書き込み)の前に確かめる
    // 取り込み後に承認まで行うので、承認の権限も先に確かめる(取り込みだけ済んで承認が拒否される、を防ぐ)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetEdit)
        || !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetApprove)
    {
        return crate::presentation::http_util::forbidden();
    }
    let order_stamp = match crate::infrastructure::repositories::stamp_repo::of_received_order(&pool, body.order_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("担当スタンプの取得に失敗: {e:#}");
            return crate::presentation::http_util::forbidden();
        }
    };
    if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetEdit, &order_stamp) {
        return crate::presentation::http_util::forbidden();
    }
    let timesheet_id = match timesheet_matching::import_attachment(&pool, id, body.order_id).await {
        Ok(tid) => tid,
        Err(msg) => {
            tracing::warn!("api_attachment_import: attachment_id={id} {msg}");
            return bad_request(&msg);
        }
    };
    // 承認（承認通知メールなどの副作用は、稼働報告の承認APIと同じ処理を使う）
    let approve_res = super::timesheets::approve(State(pool.clone()), axum::Extension(auth_user.clone()), authz.clone(), Path(timesheet_id)).await.into_response();
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
    let mut body = serde_json::json!({ "success": true, "timesheet_id": timesheet_id });
    if let Some(h) = crate::domain::services::billing_hint::billing_hint_for_timesheet(&pool, timesheet_id).await {
        body["billing_hint"] = serde_json::json!(h);
    }
    axum::Json(body).into_response()
}

#[derive(Debug, serde::Deserialize)]
pub struct RejectBody {
    #[serde(default)]
    pub reason: String,
}

/// POST /api/v1/timesheet-attachments/{id}/reject — 勤務表を差し戻す（理由を記録。メールは送らない）
pub async fn api_attachment_reject(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<RejectBody>,
) -> Response {
    // 担当スタンプを持たないデータ。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetEdit) {
        return crate::presentation::http_util::forbidden();
    }
    match timesheet_matching::reject_attachment(&pool, id, &body.reason).await {
        Ok(()) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(msg) => {
            tracing::warn!("api_attachment_reject: attachment_id={id} {msg}");
            bad_request(&msg)
        }
    }
}

/// POST /api/v1/timesheet-attachments/{id}/unreject — 差し戻しを取り消す
pub async fn api_attachment_unreject(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Path(id): Path<i64>,
) -> Response {
    // 担当スタンプを持たないデータ。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::TimesheetEdit) {
        return crate::presentation::http_util::forbidden();
    }
    match timesheet_matching::unreject_attachment(&pool, id).await {
        Ok(()) => axum::Json(serde_json::json!({ "success": true })).into_response(),
        Err(msg) => {
            tracing::warn!("api_attachment_unreject: attachment_id={id} {msg}");
            bad_request(&msg)
        }
    }
}
