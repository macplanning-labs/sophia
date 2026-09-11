use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::api_response::AppError;

// ── フォーム ──

#[derive(Debug, serde::Deserialize)]
pub struct CreateOrderForm {
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub contract_ids: Vec<String>,
    pub target_month: String,
    pub work_start: String,
    pub work_end: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct StatusForm {
    pub status: String,
}

/// 編集フォーム用
#[derive(Debug, serde::Deserialize)]
pub struct EditReceivedOrderForm {
    pub target_month: String,
    pub order_date: Option<String>,
    pub work_start: String,
    pub work_end: String,
    pub project_name: Option<String>,
    pub client_order_number: Option<String>,
    pub remarks: Option<String>,
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub item_ids: Vec<String>,
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, String>,
}

/// HTMLフォームのチェックボックスは1つだけ選択時に文字列、複数選択時に配列として送信される。
/// 両方に対応するカスタムデシリアライザ。
pub fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct StringOrVec;

    impl<'de> de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E: de::Error>(self, value: &str) -> Result<Vec<String>, E> {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<String>, A::Error> {
            let mut v = Vec::new();
            while let Some(s) = seq.next_element::<String>()? {
                v.push(s);
            }
            Ok(v)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

// ── ハンドラ ──

/// POST /received-orders — 作成（有効契約から一括生成）
///
/// 概要設計書のデータモデル: 1注文 = 1作業員（m_client_contract ──1:N──→ t_received_order）
/// 複数契約を選択した場合は、契約ごとに個別の受注書を作成する。
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<CreateOrderForm>,
) -> Result<impl IntoResponse, AppError> {
    let target_month = match chrono::NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(Redirect::to("/received-orders/new")),
    };
    let work_start = match chrono::NaiveDate::parse_from_str(&form.work_start, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(Redirect::to("/received-orders/new")),
    };
    let work_end = match chrono::NaiveDate::parse_from_str(&form.work_end, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(Redirect::to("/received-orders/new")),
    };

    let contract_ids: Vec<i64> = form.contract_ids.iter()
        .filter_map(|s| s.parse::<i64>().ok())
        .collect();

    if contract_ids.is_empty() {
        return Ok(Redirect::to("/received-orders/new"));
    }

    // 選択された契約情報を取得
    let contracts = order_repo::find_client_contracts_for_order(&pool, &contract_ids).await?;

    let mut tx = pool.begin().await?;

    // 受注書番号の採番ベース（RO-YYYYMM-NNN）
    let month_str = target_month.format("%Y%m").to_string();
    let existing_count = order_repo::count_received_orders_with_prefix(&mut tx, &format!("RO-{}-%", month_str))
        .await?;

    // 契約ごとに1受注 + 1明細を作成（1注文 = 1作業員）
    for (i, c) in contracts.iter().enumerate() {
        let received_order_no = format!("RO-{}-{:03}", month_str, existing_count + i as i64 + 1);

        // 受注書を作成
        let order_id = order_repo::insert_received_order_basic(
            &mut tx, &received_order_no, c.client_id, target_month, work_start, work_end, &c.project_name,
        ).await?;

        // 明細を1件作成（契約の精算条件をスナップショット保存）
        order_repo::insert_received_order_item(
            &mut tx, order_id, c.id, &c.engineer_name, c.base_rate, c.effort, &c.settlement_type,
            c.lower_limit_hours, c.upper_limit_hours, c.fixed_hours, c.deduction_rate, c.overtime_rate, &c.mid_month_rule,
        ).await?;
    }

    tx.commit().await?;

    Ok(Redirect::to("/received-orders"))
}

/// POST /received-orders/{id}/status — ステータス更新
pub async fn update_status(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<StatusForm>,
) -> Result<impl IntoResponse, AppError> {
    let valid = ["REGISTERED", "REPORT_RECEIVED", "REPORT_SENT", "INVOICED", "PAID"];
    if !valid.contains(&form.status.as_str()) {
        return Ok(Redirect::to(&format!("/received-orders/{}", id)));
    }

    order_repo::update_received_order_status_by_id(&pool, id, &form.status).await?;

    Ok(Redirect::to(&format!("/received-orders/{}", id)))
}

/// POST /received-orders/{id}/edit — 更新（未ルーティングのdead code。相当機能は`api::api_update`）
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<EditReceivedOrderForm>,
) -> Result<impl IntoResponse, AppError> {
    let target_month = chrono::NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d").ok();
    let order_date = form.order_date.as_deref()
        .and_then(|s| if s.is_empty() { None } else { chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok() });
    let work_start = chrono::NaiveDate::parse_from_str(&form.work_start, "%Y-%m-%d").ok();
    let work_end = chrono::NaiveDate::parse_from_str(&form.work_end, "%Y-%m-%d").ok();

    if let (Some(tm), Some(ws), Some(we)) = (target_month, work_start, work_end) {
        order_repo::update_received_order_header(
            &pool, id, tm, ws, we, order_date.unwrap_or(tm),
            form.project_name.as_deref().unwrap_or(""),
            form.client_order_number.as_deref().unwrap_or(""),
            form.remarks.as_deref().unwrap_or(""),
        ).await?;
    }

    // 明細を更新
    for item_id_str in &form.item_ids {
        let item_id: i64 = match item_id_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let unit_price: i32 = form.fields.get(&format!("unit_price_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or(0);
        let man_month: rust_decimal::Decimal = form.fields.get(&format!("man_month_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or_default();
        let actual_hours: rust_decimal::Decimal = form.fields.get(&format!("actual_hours_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or_default();
        let adjustment: i32 = form.fields.get(&format!("adjustment_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or(0);
        let amount: i32 = form.fields.get(&format!("amount_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or(0);

        order_repo::update_received_order_item_fields(
            &pool, item_id, id, unit_price, man_month, actual_hours, adjustment, amount,
        ).await?;
    }

    Ok(Redirect::to(&format!("/received-orders/{}", id)))
}

/// POST /received-orders/{id}/delete — 削除（REGISTEREDのみ）
pub async fn delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let status = order_repo::find_received_order_status(&pool, id).await.ok().flatten();

    match status.as_deref() {
        Some("REGISTERED") | None => {}
        _ => {
            tracing::warn!("受注削除（legacy）: REGISTERED以外のため拒否 id={}, status={:?}", id, status);
            return Ok(Redirect::to("/received-orders"));
        }
    }

    order_repo::delete_received_order(&pool, id).await?;
    tracing::info!("受注削除（legacy）: id={}", id);

    Ok(Redirect::to("/received-orders"))
}
