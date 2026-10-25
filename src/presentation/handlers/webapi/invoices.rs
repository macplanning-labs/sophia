/// presentation/handlers/webapi/invoices.rs — 請求書API エンドポイント

use axum::{
    extract::{Path, Extension, Query, State},
    http::StatusCode,
    Json,
};
use crate::infrastructure::db_tx::LogErr;
use serde::Deserialize;
use sqlx::PgPool;
use chrono::NaiveDate;
use sha2::{Digest, Sha256};
use utoipa::{ToSchema, IntoParams};

use crate::presentation::middleware::api_key_auth::AuthApiKey;
use crate::infrastructure::repositories::{api_access_log_repo, billing_repo, client_repo, company_info_repo, order_repo};
use crate::domain::services::jp_pint_mapper;
use super::dto::{ApiInvoiceResponse, ApiErrorResponse, map_jp_pint_to_api};
use super::util::extract_client_ip;

/// APIアクセスログを1件記録する。応答は妨げない（記録失敗は応答に影響させず、ERRORログに残す）。
/// 500系の応答は記録しない（DBエラー時は記録自体も失敗しやすいため）。
/// 記録するのはキーID・操作・表とID・状態コード・IPだけ（金額や取引先名は書かない）。
async fn log_access(
    pool: &PgPool,
    auth: &AuthApiKey,
    action: &str,
    related_id: &str,
    status: StatusCode,
    ip: &str,
) {
    if status.is_server_error() {
        return;
    }
    if let Err(e) = api_access_log_repo::insert_log(
        pool,
        auth.id,
        action,
        "t_billing_invoice",
        related_id,
        status.as_u16() as i16,
        ip,
    ).await {
        tracing::error!(
            "[WebAPI/アクセスログ] 処理=記録 結果=失敗 影響=利用記録が欠落 | action={} {:?}",
            action, e
        );
    }
}

/// 応答の状態コードを取り出す（成功は200）
fn status_of<T>(result: &Result<T, (StatusCode, Json<ApiErrorResponse>)>) -> StatusCode {
    match result {
        Ok(_) => StatusCode::OK,
        Err((status, _)) => *status,
    }
}

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
    headers: axum::http::HeaderMap,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<std::net::SocketAddr>,
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

    let company = match company_info_repo::get_company_info_basic(&pool).await {
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

    let ip = extract_client_ip(&headers, Some(&addr));
    log_access(&pool, &auth, "LIST_INVOICES", "", StatusCode::OK, &ip).await;

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
    let result = get_invoice_inner(&pool, &auth, id).await;
    let ip = extract_client_ip(&headers, Some(&addr));
    log_access(&pool, &auth, "GET_INVOICE", &id.to_string(), status_of(&result), &ip).await;
    result
}

/// GET /v1/invoices/{id} の本体（利用記録は呼び出し元の `get_invoice` で行う）
async fn get_invoice_inner(
    pool: &PgPool,
    auth: &AuthApiKey,
    id: i64,
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
        .log_err().unwrap_or_default();
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
    let company = company_info_repo::get_company_info_basic(&pool).await
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
    let result = accept_invoice_inner(&pool, &auth, id).await;
    let ip = extract_client_ip(&headers, Some(&addr));
    log_access(&pool, &auth, "ACCEPT_INVOICE", &id.to_string(), status_of(&result), &ip).await;
    result
}

/// POST /v1/invoices/{id}/accept の本体（利用記録は呼び出し元の `accept_invoice` で行う）
async fn accept_invoice_inner(
    pool: &PgPool,
    auth: &AuthApiKey,
    id: i64,
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
        .log_err().unwrap_or_default();
    // 作れなければ承諾しない(空の PDF のハッシュを記録しない)
    let pdf_bytes = match crate::presentation::handlers::invoices::pdf::client_invoice_pdf_bytes(&pool, &invoice, &client_name).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("請求書PDF生成エラー(承諾): {e}");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiErrorResponse {
                    error: "pdf_failed".to_string(),
                    message: Some("請求書PDFを作れないため、承諾できません".to_string()),
                }),
            ));
        }
    };

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

    Ok(StatusCode::OK)
}
