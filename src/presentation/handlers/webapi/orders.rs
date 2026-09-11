/// presentation/handlers/webapi/orders.rs — 発注データAPI エンドポイント

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use crate::presentation::middleware::api_key_auth::AuthApiKey;
use crate::infrastructure::repositories::{api_access_log_repo, order_repo};
use super::dto::{ApiOrderSubmitRequest, ApiOrderSubmitResponse, ApiErrorResponse};
use super::util::extract_client_ip;

/// POST /v1/orders — 発注データ送信
#[utoipa::path(
    post,
    path = "/v1/orders",
    request_body = ApiOrderSubmitRequest,
    responses(
        (status = 201, description = "発注受付完了", body = ApiOrderSubmitResponse),
        (status = 400, description = "不正なパラメータ", body = ApiErrorResponse),
        (status = 403, description = "認可不可", body = ApiErrorResponse),
        (status = 500, description = "サーバーエラー", body = ApiErrorResponse),
    ),
    security(("ApiKeyAuth" = []))
)]
pub async fn submit_order(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthApiKey>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
    Json(payload): Json<ApiOrderSubmitRequest>,
) -> Result<(StatusCode, axum::Json<ApiOrderSubmitResponse>), (StatusCode, axum::Json<ApiErrorResponse>)> {
    if auth.scope != "READ_WRITE" {
        return Err((
            StatusCode::FORBIDDEN,
            axum::Json(ApiErrorResponse {
                error: "insufficient_scope".to_string(),
                message: Some("This API key does not have READ_WRITE scope".to_string()),
            }),
        ));
    }

    if auth.party_type != "CLIENT" {
        return Err((
            StatusCode::FORBIDDEN,
            axum::Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("This API key is not for CLIENT".to_string()),
            }),
        ));
    }

    let client_id: i64 = auth.party_id.parse()
        .map_err(|_| (
            StatusCode::FORBIDDEN,
            axum::Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("Invalid party_id".to_string()),
            }),
        ))?;

    if payload.items.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            axum::Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("items cannot be empty".to_string()),
            }),
        ));
    }

    if payload.work_start > payload.work_end {
        return Err((
            StatusCode::BAD_REQUEST,
            axum::Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("work_start must be before work_end".to_string()),
            }),
        ));
    }

    let year = payload.order_date.format("%Y").to_string().parse::<i32>().unwrap_or(2026);
    let month = payload.order_date.format("%m").to_string().parse::<u32>().unwrap_or(1);
    let target_month = NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            axum::Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("Invalid order_date".to_string()),
            }),
        ))?;

    let project_name = payload.items.get(0)
        .map(|item| item.description.chars().take(255).collect::<String>())
        .unwrap_or_else(|| "WebAPI受注".to_string());

    let mut tx = pool.begin().await
        .map_err(|e| {
            tracing::error!("Failed to begin transaction: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to create order".to_string()),
                }),
            )
        })?;

    let prefix = format!("RO-{}{:02}-", year, month);
    let count = order_repo::count_received_orders_with_prefix(&mut *tx, &format!("{}%", prefix)).await
        .unwrap_or(0);
    let order_no = format!("{}{:03}", prefix, count + 1);

    let order_id = order_repo::insert_received_order_basic(
        &mut *tx,
        &order_no,
        client_id,
        target_month,
        payload.work_start,
        payload.work_end,
        &project_name,
    ).await
        .map_err(|e| {
            tracing::error!("Failed to insert received order: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to create order".to_string()),
                }),
            )
        })?;

    for item in payload.items {
        let engineer_id: i64 = item.engineer_id.parse().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(ApiErrorResponse {
                    error: "invalid_parameter".to_string(),
                    message: Some(format!("invalid engineer_id: {}", item.engineer_id)),
                }),
            )
        })?;

        let engineer_name = order_repo::find_engineer_name_by_id(&pool, engineer_id)
            .await
            .map_err(|e| {
                tracing::error!("Failed to find engineer name: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(ApiErrorResponse {
                        error: "database_error".to_string(),
                        message: Some("Failed to resolve engineer".to_string()),
                    }),
                )
            })?
            .unwrap_or_else(|| item.description.clone());

        let quantity = Decimal::from(item.quantity);
        let amount = item.unit_price * item.quantity;

        order_repo::insert_received_order_item_minimal(
            &mut *tx,
            order_id,
            &engineer_name,
            item.unit_price,
            quantity,
            quantity,
            amount,
        ).await
            .map_err(|e| {
                tracing::error!("Failed to insert received order item: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(ApiErrorResponse {
                        error: "database_error".to_string(),
                        message: Some("Failed to create order".to_string()),
                    }),
                )
            })?;
    }

    if let Some(remarks) = payload.remarks.as_deref() {
        if !remarks.is_empty() {
            order_repo::update_received_order_remarks(&mut *tx, order_id, remarks)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to update received order remarks: {:?}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(ApiErrorResponse {
                            error: "database_error".to_string(),
                            message: Some("Failed to save remarks".to_string()),
                        }),
                    )
                })?;
        }
    }

    tx.commit().await
        .map_err(|e| {
            tracing::error!("Failed to commit transaction: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to create order".to_string()),
                }),
            )
        })?;

    let ip = extract_client_ip(&headers, Some(&addr));
    if let Err(e) = api_access_log_repo::insert_log(
        &pool,
        auth.id,
        "POST_ORDER",
        "t_received_order",
        &order_id.to_string(),
        201,
        &ip,
    ).await {
        tracing::error!("Failed to insert access log: {:?}", e);
    }

    let response = ApiOrderSubmitResponse {
        received_order_id: order_id,
        received_order_no: order_no,
    };

    Ok((StatusCode::CREATED, axum::Json(response)))
}
