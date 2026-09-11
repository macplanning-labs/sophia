/// invoices/api.rs — SPA用 JSON API（一覧・詳細）

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use sqlx::PgPool;

use crate::infrastructure::repositories::billing_repo::{self, InvoiceRow};
use crate::infrastructure::repositories::order_repo;

// ── フィルタ ──

#[derive(Debug, serde::Deserialize, Default)]
pub struct InvoiceFilter {
    pub client: Option<String>,
}

/// GET /api/invoices — 一覧（JSON）
pub async fn api_index(State(pool): State<PgPool>) -> axum::Json<Vec<InvoiceRow>> {
    let rows = billing_repo::list_invoice_rows(&pool).await
        .unwrap_or_else(|e| { tracing::warn!("invoices api: {:?}", e); vec![] });
    axum::Json(rows)
}

/// GET /api/invoices/{id} — 詳細（JSON）
pub async fn api_detail(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let invoice = billing_repo::find_invoice(&pool, id).await.ok().flatten();

    match invoice {
        Some(inv) => {
            let items = billing_repo::list_invoice_items(&pool, id).await.unwrap_or_default();
            // t_billing_invoice_item に adjustment 列が無いため、精算額−基本単価で復元して返す
            let items: Vec<serde_json::Value> = items.into_iter().map(|item| {
                let adjustment = item.amount - item.unit_price;
                let mut v = serde_json::to_value(&item).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("adjustment".into(), serde_json::json!(adjustment));
                }
                v
            }).collect();

            let client_name = order_repo::find_client_name(&pool, inv.client_id).await.unwrap_or_default();

            // 入金記録
            let payments: Vec<serde_json::Value> = billing_repo::list_invoice_payment_rows(&pool, id).await.unwrap_or_default()
            .iter().map(|r| serde_json::json!({
                "payment_date": r.0.to_string(), "amount": r.1, "method": r.2, "reference": r.3,
            })).collect();

            // 関連受注書 + 案件名
            let (received_order_no, project_name): (Option<String>, Option<String>) = if let Some(ro_id) = inv.received_order_id {
                let row = billing_repo::find_received_order_no_project_name(&pool, ro_id).await.ok().flatten();
                match row {
                    Some((no, name)) => (Some(no), if name.is_empty() { None } else { Some(name) }),
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            // ヘッダに保存された金額を使用（明細集計ではなく）
            let total_amount = inv.subtotal as i64;
            let tax_amount = inv.tax_amount as i64;
            let grand_total = inv.total as i64;
            let (invoice_to_email, _) = billing_repo::resolve_invoice_send_recipients(
                &pool, inv.client_id, inv.received_order_id,
            ).await.unwrap_or((None, None));

            axum::Json(serde_json::json!({
                "invoice": inv,
                "items": items,
                "client_name": client_name,
                "project_name": project_name,
                "total_amount": total_amount,
                "tax_amount": tax_amount,
                "grand_total": grand_total,
                "payments": payments,
                "received_order_no": received_order_no,
                "invoice_to_email": invoice_to_email,
                "notes": "",
            })).into_response()
        }
        None => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
