/// presentation/handlers/webapi/invoices.rs — 請求書API エンドポイント

use axum::{
    extract::{Path, Extension, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;
use chrono::NaiveDate;
use sha2::{Digest, Sha256};
use utoipa::{ToSchema, IntoParams};

use crate::presentation::middleware::api_key_auth::AuthApiKey;
use crate::infrastructure::repositories::{api_access_log_repo, billing_repo, client_repo, company_info_repo, order_repo};
use crate::domain::services::jp_pint_mapper;
use crate::domain::services::pdf_generator::PdfGenerator;
use super::dto::{ApiInvoiceResponse, ApiErrorResponse, map_jp_pint_to_api};
use super::util::extract_client_ip;

/// 請求書一覧クエリパラメータ
#[derive(Debug, Deserialize, ToSchema, IntoParams)]
pub struct InvoiceListQuery {
    pub from: Option<String>,      // YYYY-MM-DD形式
    pub to: Option<String>,        // YYYY-MM-DD形式
    pub min_amount: Option<i32>,
    pub max_amount: Option<i32>,
}

/// GET /v1/invoices — 請求書一覧（クライアント向け）
#[utoipa::path(
    get,
    path = "/v1/invoices",
    params(InvoiceListQuery),
    responses(
        (status = 200, description = "請求書一覧", body = Vec<ApiInvoiceResponse>),
        (status = 403, description = "認可不可", body = ApiErrorResponse),
        (status = 500, description = "サーバーエラー", body = ApiErrorResponse),
    ),
    security(("ApiKeyAuth" = []))
)]
pub async fn list_invoices(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthApiKey>,
    Query(params): Query<InvoiceListQuery>,
) -> Result<Json<Vec<ApiInvoiceResponse>>, (StatusCode, Json<ApiErrorResponse>)> {
    if auth.party_type != "CLIENT" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("This API key is not for CLIENT".to_string()),
            }),
        ));
    }

    let client_id: i64 = auth.party_id.parse()
        .map_err(|_| (
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("Invalid party_id".to_string()),
            }),
        ))?;

    let from = params.from.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());
    let to = params.to.and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok());

    if (from.is_some() || to.is_some()) && from.is_some() && to.is_some() && from > to {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiErrorResponse {
                error: "invalid_parameter".to_string(),
                message: Some("from date must be before to date".to_string()),
            }),
        ));
    }

    let company = match company_info_repo::get_company_info(&pool).await {
        Ok(Some(co)) => co,
        Ok(None) => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiErrorResponse {
                    error: "unprocessable".to_string(),
                    message: Some("Company information not configured".to_string()),
                }),
            ));
        }
        Err(e) => {
            tracing::error!("get_company_info failed: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to fetch company information".to_string()),
                }),
            ));
        }
    };

    let filter = billing_repo::WebApiInvoiceFilter {
        from,
        to,
        min_amount: params.min_amount,
        max_amount: params.max_amount,
    };

    let invoices = billing_repo::list_invoices_for_client(&pool, client_id, &filter).await
        .map_err(|e| {
            tracing::error!("list_invoices_for_client failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to fetch invoices".to_string()),
                }),
            )
        })?;

    let mut results = Vec::new();
    for invoice in invoices {
        let items = match billing_repo::list_invoice_items(&pool, invoice.id).await {
            Ok(items) => items,
            Err(e) => {
                tracing::error!("list_invoice_items failed: {:?}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiErrorResponse {
                        error: "database_error".to_string(),
                        message: Some("Failed to fetch invoice items".to_string()),
                    }),
                ));
            }
        };

        let client = match client_repo::find_by_id(&pool, invoice.client_id).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                tracing::warn!(
                    "[webapi/list_invoices] 処理=請求書マッピング 結果=スキップ 影響=一覧から除外 | invoice_id={} client_id=欠落",
                    invoice.id
                );
                continue;
            }
            Err(e) => {
                tracing::error!("find_by_id failed: {:?}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiErrorResponse {
                        error: "database_error".to_string(),
                        message: Some("Failed to fetch client information".to_string()),
                    }),
                ));
            }
        };

        let own_participant_id = "0115:1000000000123";
        let jp_invoice = jp_pint_mapper::map_sales_invoice(&invoice, &items, &client, &company, &own_participant_id);
        let api_invoice = map_jp_pint_to_api(&jp_invoice, &invoice.status, invoice.client_accepted_at);
        results.push(api_invoice);
    }

    Ok(Json(results))
}

/// GET /v1/invoices/{id} — 請求書詳細
#[utoipa::path(
    get,
    path = "/v1/invoices/{id}",
    params(
        ("id" = i64, Path, description = "請求書ID"),
    ),
    responses(
        (status = 200, description = "請求書詳細", body = ApiInvoiceResponse),
        (status = 403, description = "認可不可", body = ApiErrorResponse),
        (status = 404, description = "見つかりません", body = ApiErrorResponse),
        (status = 500, description = "サーバーエラー", body = ApiErrorResponse),
    ),
    security(("ApiKeyAuth" = []))
)]
pub async fn get_invoice(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthApiKey>,
    Path(id): Path<i64>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Result<Json<ApiInvoiceResponse>, (StatusCode, Json<ApiErrorResponse>)> {
    if auth.party_type != "CLIENT" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("This API key is not for CLIENT".to_string()),
            }),
        ));
    }

    let client_id: i64 = auth.party_id.parse()
        .map_err(|_| (
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("Invalid party_id".to_string()),
            }),
        ))?;

    let invoice = billing_repo::find_invoice_for_client(&pool, id, client_id).await
        .map_err(|e| {
            tracing::error!("find_invoice_for_client failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to fetch invoice".to_string()),
                }),
            )
        })?
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ApiErrorResponse {
                error: "not_found".to_string(),
                message: Some("Invoice not found".to_string()),
            }),
        ))?;

    let items = billing_repo::list_invoice_items(&pool, invoice.id).await
        .unwrap_or_default();
    let client = client_repo::find_by_id(&pool, invoice.client_id).await
        .ok()
        .flatten()
        .ok_or_else(|| (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiErrorResponse {
                error: "unprocessable".to_string(),
                message: Some("Client information not found".to_string()),
            }),
        ))?;
    let company = company_info_repo::get_company_info(&pool).await
        .ok()
        .flatten()
        .ok_or_else(|| (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiErrorResponse {
                error: "unprocessable".to_string(),
                message: Some("Company information not configured".to_string()),
            }),
        ))?;

    let own_participant_id = "0115:1000000000123";
    let jp_invoice = jp_pint_mapper::map_sales_invoice(&invoice, &items, &client, &company, &own_participant_id);
    let api_invoice = map_jp_pint_to_api(&jp_invoice, &invoice.status, invoice.client_accepted_at);

    let ip = extract_client_ip(&headers, Some(&addr));
    if let Err(e) = api_access_log_repo::insert_log(
        &pool,
        auth.id,
        "GET_INVOICE",
        "t_billing_invoice",
        &id.to_string(),
        200,
        &ip,
    ).await {
        tracing::error!("Failed to insert access log: {:?}", e);
    }

    Ok(Json(api_invoice))
}

/// POST /v1/invoices/{id}/accept — 承諾
#[utoipa::path(
    post,
    path = "/v1/invoices/{id}/accept",
    params(
        ("id" = i64, Path, description = "請求書ID"),
    ),
    responses(
        (status = 200, description = "承諾完了"),
        (status = 403, description = "認可不可", body = ApiErrorResponse),
        (status = 404, description = "見つかりません", body = ApiErrorResponse),
        (status = 409, description = "既に承諾済み", body = ApiErrorResponse),
        (status = 500, description = "サーバーエラー", body = ApiErrorResponse),
    ),
    security(("ApiKeyAuth" = []))
)]
pub async fn accept_invoice(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthApiKey>,
    Path(id): Path<i64>,
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
) -> Result<StatusCode, (StatusCode, Json<ApiErrorResponse>)> {
    if auth.scope != "READ_WRITE" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "insufficient_scope".to_string(),
                message: Some("This API key does not have READ_WRITE scope".to_string()),
            }),
        ));
    }

    if auth.party_type != "CLIENT" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("This API key is not for CLIENT".to_string()),
            }),
        ));
    }

    let client_id: i64 = auth.party_id.parse()
        .map_err(|_| (
            StatusCode::FORBIDDEN,
            Json(ApiErrorResponse {
                error: "forbidden".to_string(),
                message: Some("Invalid party_id".to_string()),
            }),
        ))?;

    let invoice = billing_repo::find_invoice_for_client(&pool, id, client_id).await
        .map_err(|e| {
            tracing::error!("find_invoice_for_client failed: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "database_error".to_string(),
                    message: Some("Failed to fetch invoice".to_string()),
                }),
            )
        })?
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ApiErrorResponse {
                error: "not_found".to_string(),
                message: Some("Invoice not found".to_string()),
            }),
        ))?;

    if invoice.client_accepted_at.is_some() {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiErrorResponse {
                error: "already_accepted".to_string(),
                message: Some("This invoice has already been accepted".to_string()),
            }),
        ));
    }

    let client_name = order_repo::find_client_name(&pool, invoice.client_id).await
        .unwrap_or_default();
    let pdf_data = crate::presentation::handlers::invoices::pdf::build_client_invoice_pdf_data(&pool, &invoice, &client_name).await;
    let gen = PdfGenerator::new();
    let pdf_bytes = gen.generate_invoice_pdf(&pdf_data).unwrap_or_default();

    let mut hasher = Sha256::new();
    hasher.update(&pdf_bytes);
    let document_hash = format!("{:x}", hasher.finalize());

    if let Err(e) = billing_repo::confirm_client_invoice(&pool, invoice.id, &document_hash).await {
        tracing::error!("Failed to confirm invoice: {:?}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiErrorResponse {
                error: "database_error".to_string(),
                message: Some("Failed to update invoice".to_string()),
            }),
        ));
    }

    if let Some(ro_id) = invoice.received_order_id {
        if let Err(e) = order_repo::mark_received_order_invoice_confirmed(&pool, ro_id).await {
            tracing::error!("Failed to mark received order: {:?}", e);
        }
    }

    let ip = extract_client_ip(&headers, Some(&addr));
    if let Err(e) = api_access_log_repo::insert_log(
        &pool,
        auth.id,
        "ACCEPT_INVOICE",
        "t_billing_invoice",
        &id.to_string(),
        200,
        &ip,
    ).await {
        tracing::error!("Failed to insert access log: {:?}", e);
    }

    Ok(StatusCode::OK)
}
