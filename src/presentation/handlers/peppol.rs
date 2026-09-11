/// presentation/handlers/peppol.rs — Peppol送信・送受信ログ（Admin限定）
///
/// ## エンドポイント
/// - POST /api/v1/invoices/{id}/peppol/send — 売上請求書をPeppol経由で送信
/// - POST /api/v1/notices/{id}/peppol/send  — 支払通知書（セルフビリング）をPeppol経由で送信
/// - GET  /api/v1/peppol/transmissions      — 送受信ログ一覧（既定は未処理のみ）

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use sqlx::PgPool;

use crate::domain::services::jp_pint_mapper;
use crate::infrastructure::peppol_client::PeppolClient;
use crate::infrastructure::peppol_importer::{self, PeppolInboundPayload};
use crate::infrastructure::repositories::{billing_repo, client_repo, company_info_repo, order_repo, partner_repo, peppol_repo};

fn err_json(msg: impl Into<String>) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "success": false, "error": msg.into() }))
}

/// POST /api/v1/invoices/{id}/peppol/send
pub async fn send_invoice(State(pool): State<PgPool>, Path(id): Path<i64>) -> impl IntoResponse {
    let invoice = match billing_repo::find_invoice(&pool, id).await {
        Ok(Some(inv)) => inv,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, err_json("請求書が見つかりません")).into_response(),
        Err(e) => { tracing::error!("peppol send_invoice: find_invoice failed: {:?}", e); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("内部エラー")).into_response(); }
    };
    let items = billing_repo::list_invoice_items(&pool, id).await.unwrap_or_default();
    let client = match client_repo::find_by_id(&pool, invoice.client_id).await {
        Ok(Some(c)) => c,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("クライアント情報が見つかりません")).into_response(),
    };
    let company = match company_info_repo::get_company_info(&pool).await {
        Ok(Some(c)) => c,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("自社情報が未設定です")).into_response(),
    };

    let own_participant_id = PeppolClient::own_participant_id();
    let payload = jp_pint_mapper::map_sales_invoice(&invoice, &items, &client, &company, &own_participant_id);

    let client_res = PeppolClient::from_env();
    let result = match &client_res {
        Ok(c) => c.send_invoice(&payload).await,
        Err(e) => Err(match e { crate::infrastructure::peppol_client::PeppolError::NotConfigured => crate::infrastructure::peppol_client::PeppolError::NotConfigured, other => crate::infrastructure::peppol_client::PeppolError::ApiError(other.to_string()) }),
    };

    let request_json = serde_json::to_value(&payload).ok();
    match result {
        Ok(resp) => {
            if let Err(e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "INVOICE", "t_billing_invoice", &id.to_string(),
                &resp.message_id, &client.peppol_participant_id, "SENT",
                request_json, serde_json::to_value(&resp).ok(), "",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 invoice_id={} message_id={} | {}", id, resp.message_id, e);
            }
            axum::Json(serde_json::json!({ "success": true, "message_id": resp.message_id, "status": resp.status })).into_response()
        }
        Err(e) => {
            tracing::error!("[Peppol] 処理=請求書送信 影響=送信失敗 invoice_id={} | {}", id, e);
            if let Err(log_e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "INVOICE", "t_billing_invoice", &id.to_string(),
                "", &client.peppol_participant_id, "FAILED",
                request_json, None, "送信失敗",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 invoice_id={} | {}", id, log_e);
            }
            (axum::http::StatusCode::BAD_GATEWAY, err_json("Peppol送信に失敗しました")).into_response()
        }
    }
}

/// POST /api/v1/notices/{id}/peppol/send
pub async fn send_notice(State(pool): State<PgPool>, Path(id): Path<String>) -> impl IntoResponse {
    let notice = match order_repo::find_payment_notice(&pool, &id).await {
        Ok(Some(n)) => n,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, err_json("支払通知書が見つかりません")).into_response(),
        Err(e) => { tracing::error!("peppol send_notice: find_payment_notice failed: {:?}", e); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("内部エラー")).into_response(); }
    };
    let items = order_repo::list_payment_notice_items(&pool, &id).await.unwrap_or_default();

    let mut item_names = Vec::with_capacity(items.len());
    for item in &items {
        let name = order_repo::find_engineer_name_for_partner_contract(&pool, item.partner_contract_id)
            .await.ok().flatten().unwrap_or_default();
        item_names.push(name);
    }

    let partner = match partner_repo::find_by_id(&pool, &notice.partner_id).await {
        Ok(Some(p)) => p,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("パートナー情報が見つかりません")).into_response(),
    };
    let company = match company_info_repo::get_company_info(&pool).await {
        Ok(Some(c)) => c,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("自社情報が未設定です")).into_response(),
    };

    let own_participant_id = PeppolClient::own_participant_id();
    let payload = jp_pint_mapper::map_self_billing_notice(&notice, &items, &item_names, &partner, &company, &own_participant_id);

    let client_res = PeppolClient::from_env();
    let result = match &client_res {
        Ok(c) => c.send_self_billing(&payload).await,
        Err(e) => Err(match e { crate::infrastructure::peppol_client::PeppolError::NotConfigured => crate::infrastructure::peppol_client::PeppolError::NotConfigured, other => crate::infrastructure::peppol_client::PeppolError::ApiError(other.to_string()) }),
    };

    let request_json = serde_json::to_value(&payload).ok();
    match result {
        Ok(resp) => {
            if let Err(e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "SELF_BILLING", "t_payment_notice", &id,
                &resp.message_id, &partner.peppol_participant_id, "SENT",
                request_json, serde_json::to_value(&resp).ok(), "",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 notice_id={} message_id={} | {}", id, resp.message_id, e);
            }
            axum::Json(serde_json::json!({ "success": true, "message_id": resp.message_id, "status": resp.status })).into_response()
        }
        Err(e) => {
            tracing::error!("[Peppol] 処理=支払通知書送信 影響=送信失敗 notice_id={} | {}", id, e);
            if let Err(log_e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "SELF_BILLING", "t_payment_notice", &id,
                "", &partner.peppol_participant_id, "FAILED",
                request_json, None, "送信失敗",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 notice_id={} | {}", id, log_e);
            }
            (axum::http::StatusCode::BAD_GATEWAY, err_json("Peppol送信に失敗しました")).into_response()
        }
    }
}

/// POST /api/v1/peppol/inbound — Peppolプロバイダーからの受信Webhook（雛形）
/// `document_type` フィールドで文書種別を判定し処理を振り分ける。
/// - "ORDER": 発注書受信 → t_received_order新規作成
/// - それ以外（未指定含む）: 自己発行請求書（セルフビリング）受信 → t_billing_invoiceと自動突合
///
/// 外部（Peppolプロバイダー）からのサーバー間呼び出しのため、ログインセッションではなく
/// 共有シークレット（PEPPOL_WEBHOOK_SECRET + X-Peppol-Webhook-Secret ヘッダ）で認証する
/// （webhook/timesheetのWEBHOOK_SECRETと同じ方式）。
/// 本番（ENV_NAME=production）では秘密未設定を拒否する（開発標準書 §3.6）。
/// local/staging で未設定の場合のみチェックをスキップする（開発用）。
pub async fn inbound(State(pool): State<PgPool>, headers: HeaderMap, Json(body): Json<serde_json::Value>) -> impl IntoResponse {
    let env_name = std::env::var("ENV_NAME").unwrap_or_else(|_| "local".to_string());
    let secret = std::env::var("PEPPOL_WEBHOOK_SECRET").unwrap_or_default();
    if secret.is_empty() {
        if env_name == "production" {
            tracing::error!(
                "[Peppol] 処理=受信Webhook認証 結果=失敗 影響=本番で秘密未設定のため受信拒否"
            );
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                err_json("サービス設定不備"),
            )
                .into_response();
        }
        tracing::warn!(
            "[Peppol] PEPPOL_WEBHOOK_SECRET未設定のため認証スキップ（ENV_NAME={env_name}）"
        );
    } else {
        let provided = headers
            .get("X-Peppol-Webhook-Secret")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if provided != secret {
            return (axum::http::StatusCode::UNAUTHORIZED, err_json("Unauthorized")).into_response();
        }
    }

    let document_type = body.get("document_type").and_then(|v| v.as_str()).unwrap_or("");

    if document_type == "ORDER" {
        let payload: crate::infrastructure::peppol_importer::PeppolInboundOrderPayload = match serde_json::from_value(body.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[Peppol] 処理=受信発注書解析 影響=受信処理失敗 | {}", e);
                return (axum::http::StatusCode::BAD_REQUEST, err_json("ペイロード形式が無効です")).into_response();
            }
        };
        return match peppol_importer::import_inbound_order(&pool, &payload, body).await {
            Ok(result) => axum::Json(serde_json::json!({
                "success": true, "created": result.created, "received_order_id": result.received_order_id
            })).into_response(),
            Err(e) => {
                tracing::error!("[Peppol] 処理=受信発注書処理 影響=受信処理失敗 | {}", e);
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("受信処理に失敗しました")).into_response()
            }
        };
    }

    let payload: PeppolInboundPayload = match serde_json::from_value(body.clone()) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[Peppol] 処理=受信文書解析 影響=受信処理失敗 | {}", e);
            return (axum::http::StatusCode::BAD_REQUEST, err_json("ペイロード形式が無効です")).into_response();
        }
    };

    match peppol_importer::import_inbound_document(&pool, &payload, body).await {
        Ok(result) => axum::Json(serde_json::json!({
            "success": true, "matched": result.matched, "billing_invoice_id": result.billing_invoice_id
        })).into_response(),
        Err(e) => {
            tracing::error!("[Peppol] 処理=受信文書処理 影響=受信処理失敗 | {}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("受信処理に失敗しました")).into_response()
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct TransmissionFilter {
    pub status: Option<String>,
}

/// GET /api/v1/peppol/transmissions?status=all — 既定は未処理（FAILED/PENDING/UNMATCHED）のみ
pub async fn list_transmissions(State(pool): State<PgPool>, Query(filter): Query<TransmissionFilter>) -> impl IntoResponse {
    let result = if filter.status.as_deref() == Some("all") {
        peppol_repo::list_all(&pool).await
    } else {
        peppol_repo::list_actionable(&pool).await
    };

    match result {
        Ok(rows) => axum::Json(rows).into_response(),
        Err(e) => {
            tracing::error!("peppol list_transmissions failed: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("内部エラー")).into_response()
        }
    }
}
