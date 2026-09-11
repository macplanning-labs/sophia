use axum::{
    extract::{Path, State},
    response::IntoResponse,
};
use sqlx::PgPool;
use crate::infrastructure::repositories::order_repo::{self, ReceivedOrderRow};
use crate::domain::models::client_contract::ReceivedOrderStatus;
use crate::presentation::api_response::AppError;

#[derive(Debug, serde::Deserialize)]
pub struct ApiCreateOrderForm {
    pub client_contract_id: i64,
    pub target_month: String,
    pub work_start: String,
    pub work_end: String,
}

// SPA用 JSON API

/// GET /api/received-orders — 一覧（JSON）
pub async fn api_index(State(pool): State<PgPool>) -> axum::Json<Vec<ReceivedOrderRow>> {
    let rows = order_repo::list_received_order_rows(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("received_orders api: {:?}", e); vec![] });
    axum::Json(rows)
}

/// POST /api/received-orders — 受注契約から受注書を作成（JSON）
///
/// 受注契約(m_client_contract)を1件指定し、その精算条件をスナップショットした
/// 受注書(t_received_order)+受注明細(t_received_order_item)を1件作成する。
pub async fn api_create(
    State(pool): State<PgPool>,
    axum::Json(form): axum::Json<ApiCreateOrderForm>,
) -> impl IntoResponse {
    let target_month = match chrono::NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
            "error": "対象月の形式が不正です（YYYY-MM-DD）"
        }))).into_response(),
    };
    let work_start = match chrono::NaiveDate::parse_from_str(&form.work_start, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
            "error": "作業開始日の形式が不正です（YYYY-MM-DD）"
        }))).into_response(),
    };
    let work_end = match chrono::NaiveDate::parse_from_str(&form.work_end, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
            "error": "作業終了日の形式が不正です（YYYY-MM-DD）"
        }))).into_response(),
    };

    let contract = order_repo::find_client_contract_for_order(&pool, form.client_contract_id).await.ok().flatten();

    let c = match contract {
        Some(c) => c,
        None => return (axum::http::StatusCode::NOT_FOUND, axum::Json(serde_json::json!({
            "error": "受注契約が見つかりません"
        }))).into_response(),
    };

    // 同一契約・同一対象月の未キャンセル受注があれば重複作成を拒否
    let dup = match order_repo::received_order_exists_for_contract_month(
        &pool,
        form.client_contract_id,
        target_month,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(
                "[受注書/作成] 処理=重複チェック 結果=失敗 影響=重複判定不能のため作成中止 | {:?}",
                e
            );
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": "db error" })),
            )
                .into_response();
        }
    };
    if dup {
        return (
            axum::http::StatusCode::CONFLICT,
            axum::Json(serde_json::json!({
                "error": format!(
                    "{} の受注書は既に存在します",
                    target_month.format("%Y年%m月")
                )
            })),
        )
            .into_response();
    }

    let mut tx = match pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("受注書作成: トランザクション開始失敗: {:?}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({
                "error": "db error"
            }))).into_response();
        }
    };

    let month_str = target_month.format("%Y%m").to_string();
    let existing_count = order_repo::count_received_orders_with_prefix(&mut tx, &format!("RO-{}-%", month_str))
        .await
        .unwrap_or(0);
    let received_order_no = format!("RO-{}-{:03}", month_str, existing_count + 1);

    let order_id = match order_repo::insert_received_order_with_contract(
        &mut tx, &received_order_no, c.client_id, c.engineer_id, c.id, target_month, work_start, work_end, &c.project_name,
    ).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("受注書作成: INSERT失敗: {:?}", e);
            return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({
                "error": "受注書の作成に失敗しました"
            }))).into_response();
        }
    };

    if let Err(e) = order_repo::insert_received_order_item(
        &mut tx, order_id, c.id, &c.engineer_name, c.base_rate, c.effort, &c.settlement_type,
        c.lower_limit_hours, c.upper_limit_hours, c.fixed_hours, c.deduction_rate, c.overtime_rate, &c.mid_month_rule,
    ).await {
        tracing::error!("受注書作成: 明細INSERT失敗: {:?}", e);
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({
            "error": "受注明細の作成に失敗しました"
        }))).into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("受注書作成: commit失敗: {:?}", e);
        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(serde_json::json!({
            "error": "コミットに失敗しました"
        }))).into_response();
    }

    axum::Json(serde_json::json!({ "id": order_id, "received_order_no": received_order_no })).into_response()
}

/// GET /api/received-orders/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let order = order_repo::find_received_order(&pool, id).await.ok().flatten();

    match order {
        Some(order) => {
            let items = order_repo::list_received_order_items(&pool, id).await.unwrap_or_default();

            let client_name = order_repo::find_client_name(&pool, order.client_id).await.unwrap_or_default();

            let status_enum = ReceivedOrderStatus::from_str(&order.status);
            let needs_contract_link = order.client_contract_id.is_none();
            let link_candidates = if needs_contract_link {
                order_repo::list_contract_link_candidates(&pool, id).await.unwrap_or_default()
            } else {
                vec![]
            };

            axum::Json(serde_json::json!({
                "order": order,
                "items": items,
                "client_name": client_name,
                "status_display": status_enum.display(),
                "status_badge": status_enum.badge_class(),
                "needs_contract_link": needs_contract_link,
                "link_candidates": link_candidates,
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct LinkContractBody {
    pub client_contract_id: i64,
}

/// POST /api/received-orders/{id}/link-contract — オペレーターが受注契約を手動紐付け
pub async fn api_link_contract(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(body): axum::Json<LinkContractBody>,
) -> impl IntoResponse {
    // link_received_order_to_contractのエラーはanyhow::bail!によるユーザー向け業務メッセージ
    // （紐付け条件を満たさない旨の案内）であり、DB例外の生漏洩ではないためAppError化しない
    // （品質改善P2-1の対象外。他の2関数=api_delete/api_updateとは性質が異なる）。
    match order_repo::link_received_order_to_contract(&pool, id, body.client_contract_id).await {
        Ok(()) => axum::Json(serde_json::json!({
            "success": true,
            "message": "受注契約を紐付けました"
        })).into_response(),
        Err(e) => {
            tracing::warn!("受注契約紐付け失敗: id={id}, cc={}, err={e}", body.client_contract_id);
            (
                axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                    "code": "NEEDS_CONTRACT_LINK"
                })),
            )
                .into_response()
        }
    }
}

/// POST /api/received-orders/{id}/rollforward — 翌月ロールフォワード（JSON）
pub async fn api_rollforward(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    // rollforward_orderのエラーはanyhow::bail!によるユーザー向け業務メッセージ（重複月の案内）
    // を含むため、api_link_contractと同じ理由でAppError化しない（品質改善P2-1の対象外）。
    match crate::domain::services::rollforward::rollforward_order(&pool, id).await {
        Ok(new_id) => {
            let new_order = order_repo::find_received_order(&pool, new_id).await.ok().flatten();
            let received_order_no = new_order.map(|o| o.received_order_no).unwrap_or_default();
            (axum::http::StatusCode::CREATED, axum::Json(serde_json::json!({
                "success": true, "id": new_id, "received_order_no": received_order_no,
                "message": "翌月の受注書を作成しました"
            }))).into_response()
        }
        Err(e) => {
            tracing::warn!("受注書ロールフォワード失敗: {:?}", e);
            (axum::http::StatusCode::CONFLICT, axum::Json(serde_json::json!({
                "success": false, "error": format!("{e}")
            }))).into_response()
        }
    }
}

/// PUT /api/received-orders/{id} — 受注書ヘッダー・明細更新（JSON）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let fields = order_repo::ReceivedOrderUpdateFields {
        status: payload["status"].as_str(),
        target_month: payload["target_month"].as_str(),
        remarks: payload["remarks"].as_str(),
        project_name: payload["project_name"].as_str(),
        work_start: payload["work_start"].as_str(),
        work_end: payload["work_end"].as_str(),
        client_order_number: payload["client_order_number"].as_str(),
        payment_condition: payload["payment_condition"].as_str(),
        report_to_email: payload["report_to_email"].as_str(),
        report_cc_emails: payload["report_cc_emails"].as_str(),
        invoice_to_email: payload["invoice_to_email"].as_str(),
        invoice_cc_emails: payload["invoice_cc_emails"].as_str(),
    };
    let items = payload["items"].as_array();

    if fields.is_empty() && items.is_none() {
        return Ok(axum::Json(serde_json::json!({ "success": false, "error": "更新フィールドが指定されていません" })).into_response());
    }

    if !fields.is_empty() {
        order_repo::update_received_order_partial(&pool, id, &fields).await?;
    }

    if let Some(items) = items {
        for item in items {
            let item_id = match item["id"].as_i64() {
                Some(v) => v,
                None => continue,
            };
            let unit_price = item["unit_price"].as_i64().unwrap_or(0) as i32;
            let man_month: rust_decimal::Decimal = item["man_month"].as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default();
            let settlement_type = item["settlement_type"].as_str().unwrap_or("");
            let lower_limit_hours: rust_decimal::Decimal = item["lower_limit_hours"].as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default();
            let upper_limit_hours: rust_decimal::Decimal = item["upper_limit_hours"].as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default();
            let deduction_rate = item["deduction_rate"].as_i64().unwrap_or(0) as i32;
            let overtime_rate = item["overtime_rate"].as_i64().unwrap_or(0) as i32;

            // 金額再計算（insert_received_order_item/rollforwardと同じ amount = unit_price * man_month）
            let amount = (unit_price as f64 * rust_decimal::prelude::ToPrimitive::to_f64(&man_month).unwrap_or(1.0)) as i32;

            order_repo::update_received_order_item_settlement_fields(
                &pool, item_id, id, unit_price, man_month, settlement_type,
                lower_limit_hours, upper_limit_hours, deduction_rate, overtime_rate, amount,
            ).await?;
        }
    }

    Ok(axum::Json(serde_json::json!({ "success": true })).into_response())
}

/// DELETE /api/received-orders/{id} — 削除（REGISTEREDのみ、JSON）
///
/// 受注書にはDRAFTステータスが無いため、起票直後の初期状態であるREGISTEREDを
/// 「削除可能な状態」として扱う（orders/api.rs::api_deleteのDRAFT相当）。
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let status = order_repo::find_received_order_status(&pool, id).await.ok().flatten();

    match status.as_deref() {
        Some("REGISTERED") | None => {}
        _ => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false, "error": "REGISTERED状態の受注書のみ削除できます"
            }))).into_response());
        }
    }

    order_repo::delete_received_order(&pool, id).await?;

    tracing::info!("受注書削除（API）: {}", id);
    Ok(axum::Json(serde_json::json!({ "success": true, "message": "削除しました" })).into_response())
}
