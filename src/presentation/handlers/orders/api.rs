/// orders/api.rs — SPA用 JSON API（発注書 CRUD・ステータス・送付/再送付）

use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    Json,
};
use sqlx::PgPool;

use crate::domain::models::partner_contract::PurchaseOrderStatus;
use crate::infrastructure::repositories::order_repo::{self, OrderRow};
use crate::infrastructure::repositories::task_repo;
use crate::presentation::api_response::AppError;

use super::{generate_order_id, StatusForm};

/// 発注書UUIDから、ログイン不要の稼働報告アップロード付きトークンページURLを組み立てる。
fn order_token_url(order_uuid: &uuid::Uuid) -> String {
    let base_url = std::env::var("BASE_URL").unwrap_or_default();
    format!("{}/token/{}", base_url.trim_end_matches('/'), order_uuid)
}

// ── フィルタ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct OrderFilter {
    pub partner: Option<String>,
    pub status: Option<String>,
}

/// GET /api/orders — 一覧（JSON）
pub async fn api_index(
    State(pool): State<PgPool>,
    Query(filter): Query<OrderFilter>,
) -> axum::Json<Vec<OrderRow>> {
    let partner = filter.partner.unwrap_or_default();
    let status = filter.status.unwrap_or_default();

    let orders = order_repo::list_orders_filtered(&pool, &partner, &status).await
        .unwrap_or_else(|e| { tracing::warn!("orders api: {:?}", e); vec![] });

    axum::Json(orders)
}

/// GET /api/orders/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    match order {
        Some(order) => {
            let items = order_repo::list_order_items_with_engineer(&pool, &id).await
                .unwrap_or_else(|e| { tracing::warn!("orders: {:?}", e); vec![] });

            let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
            let (partner_name, partner_email) = partner_info.unwrap_or_default();

            let project_name = order_repo::find_project_name(&pool, &order.project_id).await
                .ok().flatten().unwrap_or_default();

            let status_enum = PurchaseOrderStatus::from_str(&order.status);
            let total_amount: i64 = items.iter().map(|i| i.amount as i64).sum();

            axum::Json(serde_json::json!({
                "order": order,
                "items": items,
                "partner_name": partner_name,
                "partner_email": partner_email,
                "project_name": project_name,
                "status_display": status_enum.display(),
                "status_badge": status_enum.badge_class(),
                "total_amount": total_amount,
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

// ══════════════════════════════════════════════════════════
// SPA用 JSON CRUD API
// ══════════════════════════════════════════════════════════

/// POST /api/orders/{id}/status — ステータス変更（JSON）
pub async fn api_update_status(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(form): Json<StatusForm>,
) -> Result<impl IntoResponse, AppError> {
    use crate::infrastructure::repositories::order_repo::update_purchase_order_status;

    let valid = ["DRAFT", "SENT", "ACCEPTED", "REPORT_RECEIVED",
                 "NOTICE_CREATED", "NOTICE_CONFIRMED", "PAID"];
    if !valid.contains(&form.status.as_str()) {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "無効なステータスです"
        }))).into_response());
    }

    update_purchase_order_status(&pool, &id, &form.status).await?;
    Ok(Json(serde_json::json!({ "success": true, "message": "ステータスを更新しました" })).into_response())
}

/// DELETE /api/orders/{id} — 削除（DRAFTのみ, JSON）
pub async fn api_delete(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use crate::infrastructure::repositories::order_repo::{find_purchase_order_status, delete_purchase_order};

    let status = find_purchase_order_status(&pool, &id).await.ok().flatten();

    match status.as_deref() {
        Some("DRAFT") | None => {},
        _ => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "success": false, "error": "DRAFT状態の発注書のみ削除できます"
            }))).into_response());
        }
    }

    delete_purchase_order(&pool, &id).await?;

    tracing::info!("発注書削除（API）: {}", id);
    Ok(Json(serde_json::json!({ "success": true, "message": "削除しました" })).into_response())
}

/// PUT /api/orders/{id} — 発注書更新（JSON、DRAFTのみ）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    // DRAFTチェック
    let status = order_repo::find_purchase_order_status(&pool, &id).await.ok().flatten();

    if status.as_deref() != Some("DRAFT") {
        return Ok((axum::http::StatusCode::FORBIDDEN, Json(serde_json::json!({
            "success": false, "error": "DRAFT以外の発注書は編集できません"
        }))).into_response());
    }

    // ヘッダ更新
    let work_start = body["work_start"].as_str().and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let work_end = body["work_end"].as_str().and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let work_location = body["work_location"].as_str().unwrap_or("");
    let payment_condition = body["payment_condition"].as_str();

    if let (Some(ws), Some(we)) = (work_start, work_end) {
        order_repo::update_purchase_order_work(&pool, &id, ws, we, work_location, payment_condition).await?;
    }

    // 明細更新
    if let Some(items) = body["items"].as_array() {
        for item in items {
            let item_id: i64 = match item["id"].as_i64() {
                Some(v) => v,
                None => continue,
            };
            let base_fee = item["base_fee"].as_i64().unwrap_or(0) as i32;
            let effort: rust_decimal::Decimal = item["effort"].as_str()
                .or_else(|| item["effort"].as_f64().map(|_| ""))
                .and_then(|s| if s.is_empty() { item["effort"].as_f64().map(|f| rust_decimal::Decimal::from_f64_retain(f).unwrap_or_default()) } else { s.parse().ok() })
                .unwrap_or_default();
            let settlement_type = item["settlement_type"].as_str().unwrap_or("");
            let lower_limit: rust_decimal::Decimal = item["lower_limit_hours"].as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| item["lower_limit_hours"].as_f64().map(|f| rust_decimal::Decimal::from_f64_retain(f).unwrap_or_default()))
                .unwrap_or_default();
            let upper_limit: rust_decimal::Decimal = item["upper_limit_hours"].as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| item["upper_limit_hours"].as_f64().map(|f| rust_decimal::Decimal::from_f64_retain(f).unwrap_or_default()))
                .unwrap_or_default();
            let deduction_rate = item["deduction_rate"].as_i64().unwrap_or(0) as i32;
            let overtime_rate = item["overtime_rate"].as_i64().unwrap_or(0) as i32;

            // 金額再計算
            let price = (base_fee as f64 * rust_decimal::prelude::ToPrimitive::to_f64(&effort).unwrap_or(1.0)) as i32;

            order_repo::update_purchase_order_item_fields(
                &pool, item_id, &id, base_fee, effort, settlement_type,
                lower_limit, upper_limit, deduction_rate, overtime_rate, price,
            ).await?;
        }
    }

    Ok(Json(serde_json::json!({ "success": true })).into_response())
}

/// POST /api/orders/{id}/rollforward — ロールフォワード（JSON）
pub async fn api_rollforward(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    match order {
        Some(o) => {
            let new_start = o.work_start + chrono::Months::new(1);
            let new_end = o.work_end + chrono::Months::new(1);
            let new_order_id = generate_order_id(&pool, new_start).await;

            let mut tx = pool.begin().await?;

            // 重複チェック
            let draft_exists = order_repo::draft_order_exists(
                &mut *tx, &o.partner_id, &o.project_id, o.partner_contract_id.unwrap_or(0), new_start, new_end,
            ).await?;

            if draft_exists {
                return Ok((axum::http::StatusCode::CONFLICT, Json(serde_json::json!({
                    "success": false, "error": "同一条件のDRAFT発注書が既に存在します"
                }))).into_response());
            }

            order_repo::insert_purchase_order_full(
                &mut *tx, &new_order_id, &o.partner_id, &o.project_id,
                o.partner_contract_id.unwrap_or(0), new_start, new_end,
                o.workplace_id, &o.deliverable_text, &o.payment_condition,
                &o.contract_items, &o.remarks,
            ).await?;

            // 明細コピー（失敗時はロールバックし、明細なしの発注書が201で返らないようにする）
            order_repo::copy_order_items(&mut tx, &o.order_id, &new_order_id).await?;

            tx.commit().await?;

            Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({
                "success": true, "order_id": new_order_id, "message": "翌月の発注書を作成しました"
            }))).into_response())
        }
        None => Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "発注書が見つかりません"
        }))).into_response()),
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct ApiCreateOrderForm {
    pub contract_ids: Vec<i64>,
    pub work_start: String,
    pub work_end: String,
}

/// POST /api/orders — 発注契約から発注書を新規作成（JSON）
///
/// legacy.rs::create（未ルーティングのSSRフォーム版）と同じロジック。
/// 選択された発注契約をパートナー単位でグルーピングし、パートナーごとにDRAFT発注書を1件作成する。
pub async fn api_create(
    State(pool): State<PgPool>,
    Json(form): Json<ApiCreateOrderForm>,
) -> Result<impl IntoResponse, AppError> {
    let work_start = match chrono::NaiveDate::parse_from_str(&form.work_start, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "作業開始日の形式が不正です"
        }))).into_response()),
    };
    let work_end = match chrono::NaiveDate::parse_from_str(&form.work_end, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "作業終了日の形式が不正です"
        }))).into_response()),
    };

    if form.contract_ids.is_empty() {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "発注契約が選択されていません"
        }))).into_response());
    }

    // DBエラーを「契約が見つからない」(404)と混同しないよう、失敗時はそのままAppErrorとして伝播する
    let contracts = order_repo::find_contracts_for_order(&pool, &form.contract_ids).await?;

    if contracts.is_empty() {
        return Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "指定された発注契約が見つかりません"
        }))).into_response());
    }

    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<&order_repo::ContractForOrder>> = HashMap::new();
    for c in &contracts {
        groups.entry(c.partner_id.clone()).or_default().push(c);
    }

    let mut tx = pool.begin().await?;

    let mut created_order_ids = Vec::new();
    let mut skipped_partners = Vec::new();

    // パートナーグループ単位の失敗は他グループの作成を妨げない意図的な部分成功扱い
    // （失敗はログに残し、作成できたIDのみをレスポンスで返す）
    for (partner_id, group) in &groups {
        let first = group[0];

        let draft_exists = order_repo::draft_order_exists(
            &mut *tx, partner_id, &first.project_id, first.id, work_start, work_end,
        ).await.unwrap_or(false);

        if draft_exists {
            skipped_partners.push(first.partner_name.clone());
            continue;
        }

        let order_id = generate_order_id(&pool, work_start).await;

        if let Err(e) = order_repo::insert_purchase_order(
            &mut *tx, &order_id, partner_id, &first.project_id, first.id, work_start, work_end,
        ).await {
            tracing::error!("api_create orders: ヘッダーINSERT失敗: {:?}", e);
            continue;
        }

        for c in group {
            let price = (c.base_rate as f64 * rust_decimal::prelude::ToPrimitive::to_f64(&c.effort).unwrap_or(1.0)) as i32;
            if let Err(e) = order_repo::insert_purchase_order_item(
                &mut *tx, &order_id, c.id, c.base_rate, c.effort, &c.settlement_type,
                c.lower_limit_hours, c.upper_limit_hours, c.fixed_hours,
                c.deduction_rate, c.overtime_rate, price,
            ).await {
                tracing::error!("api_create orders: 明細INSERT失敗: {:?}", e);
            }
        }

        created_order_ids.push(order_id);
    }

    tx.commit().await?;

    if created_order_ids.is_empty() {
        return Ok((axum::http::StatusCode::CONFLICT, Json(serde_json::json!({
            "success": false,
            "error": format!("同一条件のDRAFT発注書が既に存在するため作成しませんでした（{}）", skipped_partners.join(", "))
        }))).into_response());
    }

    Ok((axum::http::StatusCode::CREATED, Json(serde_json::json!({
        "success": true, "order_ids": created_order_ids, "skipped_partners": skipped_partners
    }))).into_response())
}

/// GET /api/orders/form-data — 作成フォーム用データ（JSON）
pub async fn api_form_data(State(pool): State<PgPool>) -> impl IntoResponse {
    let contracts = order_repo::list_active_contracts_for_order_form(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("api_form_data: {:?}", e); vec![] });

    Json(serde_json::json!({
        "contracts": contracts,
    }))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct SendMailBody {
    pub subject: Option<String>,
    pub body: Option<String>,
}

/// GET /api/orders/{id}/email-preview — 送信メール本文プレビュー
pub async fn api_email_preview(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::{EmailService, compose_order_publish_email};

    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    let Some(order) = order else {
        return Ok((axum::http::StatusCode::NOT_FOUND, "発注書が見つかりません").into_response());
    };

    let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
    let (partner_name, _) = partner_info.unwrap_or_default();

    let base_url = std::env::var("BASE_URL").unwrap_or_default();
    let token_url = format!("{}/token/{}", base_url, order.uuid);
    let month = order.work_start.format("%Y年%m月").to_string();
    let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(&pool).await;
    let ctx = compose_order_publish_email(&partner_name, &order.order_id, &month, &token_url, expiry_days);

    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = email_svc.render_template("order_send", &ctx).await?;
    Ok(axum::Json(serde_json::json!({ "subject": subject, "body": body })).into_response())
}

/// POST /api/orders/{id}/publish — 発注書送付（メール + ステータスSENT更新）JSON版
pub async fn api_publish(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(payload): Json<SendMailBody>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::EmailService;

    // 発注書取得
    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => return Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "発注書が見つかりません"
        }))).into_response()),
    };

    // DRAFTのみ送付可能
    if order.status != "DRAFT" {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "下書き状態の発注書のみ送付できます"
        }))).into_response());
    }

    // パートナー情報
    let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
    let (partner_name, partner_email) = partner_info.unwrap_or_default();

    if partner_email.is_empty() {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "パートナーのメールアドレスが未登録です"
        }))).into_response());
    }

    // 件名・本文: 送信モーダルで編集済みならそれを使い、無指定ならテンプレートから生成
    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = match (payload.subject, payload.body) {
        (Some(s), Some(b)) if !s.is_empty() && !b.is_empty() => (s, b),
        _ => {
            let base_url = std::env::var("BASE_URL").unwrap_or_default();
            let token_url = format!("{}/token/{}", base_url, order.uuid);
            let month = order.work_start.format("%Y年%m月").to_string();
            let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(&pool).await;
            let ctx = crate::domain::services::email_service::compose_order_publish_email(
                &partner_name, &order.order_id, &month, &token_url, expiry_days,
            );
            email_svc.render_template("order_send", &ctx).await?
        }
    };

    email_svc.send(&partner_email, None, &subject, &body).await?;

    // ステータスを SENT に更新
    order_repo::mark_purchase_order_sent(&pool, &id).await?;

    tracing::info!("発注書送付（API）: {} → {}", id, partner_email);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("{} に送付しました", partner_email)
    })).into_response())
}

/// POST /api/orders/{id}/republish — 注文書訂正再送付（メール送信）JSON版
/// EDI互換: orders/services/order_service.py republish_order
pub async fn api_republish(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::email_service::EmailService;

    // 発注書取得
    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => return Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
            "success": false, "error": "発注書が見つかりません"
        }))).into_response()),
    };

    // SENT or ACCEPTED のみ訂正可能
    if order.status != "SENT" && order.status != "ACCEPTED" {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "送付済み or 承諾済みの発注書のみ訂正再送付できます"
        }))).into_response());
    }

    // パートナー情報
    let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
    let (partner_name, partner_email) = partner_info.unwrap_or_default();

    if partner_email.is_empty() {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "パートナーのメールアドレスが未登録です"
        }))).into_response());
    }

    // 訂正メール送信
    let base_url = std::env::var("BASE_URL").unwrap_or_default();
    let order_url = format!("{}/token/{}", base_url, order.uuid);
    let ctx = crate::domain::services::email_service::compose_order_republish_email(
        &partner_name, &order.order_id, &order_url,
    );
    let email_svc = EmailService::new(pool.clone());
    if let Err(e) = email_svc.send_by_template("order_republish", &partner_email, None, &ctx).await {
        tracing::error!("訂正再送付メール送信エラー: {:?}", e);
        return Ok((axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "success": false, "error": format!("メール送信に失敗しました: {}", e)
        }))).into_response());
    }

    // ステータスをSENTに戻す（承諾済みからの訂正対応）。メール送信済みのためここでの失敗も明示的に伝える
    order_repo::mark_purchase_order_sent(&pool, &id).await?;

    tracing::info!("発注書訂正再送付（API）: {} → {}", id, partner_email);
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("{} に訂正版を送付しました", partner_email)
    })).into_response())
}

/// GET /api/orders/{id}/request-timesheet-preview — 提出依頼メール本文プレビュー
pub async fn api_request_timesheet_preview(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use chrono::Datelike;
    use crate::domain::services::email_service::{EmailService, compose_work_report_request_email};

    let order = match order_repo::find_purchase_order(&pool, &id).await.ok().flatten() {
        Some(o) => o,
        None => {
            return Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
                "error": "発注書が見つかりません"
            }))).into_response());
        }
    };

    let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
    let (partner_name, partner_email) = partner_info.unwrap_or_default();
    let month_label = order.work_start.format("%Y年%m月").to_string();
    let work_month = chrono::NaiveDate::from_ymd_opt(order.work_start.year(), order.work_start.month(), 1)
        .unwrap_or(order.work_start);
    let token_url = order_token_url(&order.uuid);
    let deadline = task_repo::resolve_report_deadline(&pool, &order.partner_id, &order.project_id, order.engineer_id, work_month).await;
    let ctx = compose_work_report_request_email(&partner_name, &month_label, &token_url, deadline);

    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = email_svc.render_template("work_report_request", &ctx).await?;
    Ok(Json(serde_json::json!({
        "subject": subject, "body": body, "to_email": partner_email, "partner_name": partner_name,
    })).into_response())
}

/// multipart/form-data から件名・本文・添付ファイル（任意）を取り出す。
/// テキストフィールドの順序は問わない（`next_field`で来た順に処理）。
async fn extract_request_timesheet_fields(
    multipart: &mut axum::extract::Multipart,
) -> (Option<String>, Option<String>, Option<crate::domain::services::email_service::EmailAttachment>) {
    let mut subject: Option<String> = None;
    let mut body: Option<String> = None;
    let mut attachment: Option<crate::domain::services::email_service::EmailAttachment> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        match field.name().map(str::to_string).as_deref() {
            Some("subject") => subject = field.text().await.ok(),
            Some("body") => body = field.text().await.ok(),
            Some("file") => {
                let filename = field.file_name().unwrap_or("attachment.pdf").to_string();
                let content_type = field.content_type().unwrap_or("application/octet-stream").to_string();
                if let Ok(bytes) = field.bytes().await {
                    if !bytes.is_empty() {
                        attachment = Some(crate::domain::services::email_service::EmailAttachment {
                            filename,
                            content: bytes.to_vec(),
                            content_type,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    (subject, body, attachment)
}

/// POST /api/orders/{id}/request-timesheet — 稼働報告書提出依頼メールをパートナーへ送信
///
/// multipart/form-data（subject, body, file（任意・クロス等から受け取った勤務表PDFの添付用））
pub async fn api_request_timesheet(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    mut multipart: axum::extract::Multipart,
) -> Result<impl IntoResponse, AppError> {
    use chrono::Datelike;
    use crate::domain::services::email_service::{EmailService, compose_work_report_request_email};

    let (payload_subject, payload_body, attachment) = extract_request_timesheet_fields(&mut multipart).await;

    let order = match order_repo::find_purchase_order(&pool, &id).await.ok().flatten() {
        Some(o) => o,
        None => {
            return Ok((axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({
                "success": false, "error": "発注書が見つかりません"
            }))).into_response());
        }
    };

    // 受諾後〜報告受領前が主な対象。DRAFT/SENTでも催促は可能にするが、報告受領済以降は拒否。
    const ALLOWED: &[&str] = &["SENT", "ACCEPTED"];
    if !ALLOWED.contains(&order.status.as_str()) {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false,
            "error": "送付済または受諾済の発注書にのみ提出依頼メールを送信できます"
        }))).into_response());
    }

    let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
    let (partner_name, partner_email) = partner_info.unwrap_or_default();

    if partner_email.is_empty() {
        return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false, "error": "パートナーのメールアドレスが未登録です"
        }))).into_response());
    }

    let month_label = order.work_start.format("%Y年%m月").to_string();
    let work_month = chrono::NaiveDate::from_ymd_opt(
        order.work_start.year(),
        order.work_start.month(),
        1,
    ).unwrap_or(order.work_start);

    // 件名・本文: 送信モーダルで編集済みならそれを使い、無指定ならテンプレートから生成
    let email_svc = EmailService::new(pool.clone());
    let (subject, body) = match (payload_subject, payload_body) {
        (Some(s), Some(b)) if !s.is_empty() && !b.is_empty() => (s, b),
        _ => {
            let token_url = order_token_url(&order.uuid);
            let deadline = task_repo::resolve_report_deadline(&pool, &order.partner_id, &order.project_id, order.engineer_id, work_month).await;
            let ctx = compose_work_report_request_email(&partner_name, &month_label, &token_url, deadline);
            email_svc.render_template("work_report_request", &ctx).await?
        }
    };

    // クロス等から受け取った勤務表PDFが添付されていれば同梱して送信（添付なしなら従来通りテキストのみ）
    let attachments = attachment.into_iter().collect::<Vec<_>>();
    email_svc.send_with_attachment(&partner_email, None, &subject, &body, attachments).await?;

    if let Err(e) = task_repo::mark_report_reminder_sent(&pool, &order.partner_id, work_month).await {
        tracing::warn!("reminder_sent更新エラー（メールは送信済）: {:?}", e);
    }

    tracing::info!(
        "稼働報告提出依頼メール送信: order={} → {} ({})",
        id, partner_email, month_label
    );

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("{} に {} 分の提出依頼メールを送信しました", partner_email, month_label),
    })).into_response())
}
