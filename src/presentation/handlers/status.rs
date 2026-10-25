//! presentation/handlers/status.rs — 月別状況(UI刷新 3-8 / DEMO-000163)
//!
//! - `GET /api/v1/status/matrix?month=YYYY-MM` — アサインごとの進み具合と次の操作(管理者専用・読み取りのみ)

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::domain::services::billing_confirm::parse_month;
use crate::domain::services::settlement_dashboard::{list_settlement_rows, SettlementFilter};
use crate::domain::services::status_matrix::build_matrix;
use crate::presentation::api_response::ApiError;

#[derive(Debug, Deserialize)]
pub struct MatrixQuery {
    /// "YYYY-MM"
    pub month: Option<String>,
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, Json(ApiError { success: false, error: message.to_string() })).into_response()
}

pub async fn api_matrix(State(pool): State<PgPool>, Query(q): Query<MatrixQuery>) -> Response {
    let Some(month) = q.month.as_deref().and_then(parse_month) else {
        return error_response(StatusCode::BAD_REQUEST, "month は YYYY-MM の形で指定してください");
    };
    match list_settlement_rows(&pool, month, &SettlementFilter::default()).await {
        Ok(rows) => {
            let (rows, summary) = build_matrix(&rows);
            Json(serde_json::json!({
                "month": month.format("%Y-%m").to_string(),
                "summary": summary,
                "rows": rows,
            }))
            .into_response()
        }
        Err(e) => {
            tracing::error!("status matrix failed: {e}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "月別状況を取得できませんでした")
        }
    }
}
