/// invoices/crud.rs — 受注からの請求書自動生成・編集・削除

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form, Json,
};
use sqlx::PgPool;
use serde::Deserialize;

use crate::domain::models::billing::InvoiceForm;
use crate::infrastructure::repositories::billing_repo;
use crate::infrastructure::repositories::order_repo;
use crate::presentation::api_response::AppError;

/// POST /invoices — 受注から請求書自動生成
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<InvoiceForm>,
) -> Result<impl IntoResponse, AppError> {
    let received_order_id = match form.received_order_id {
        Some(id) => id,
        None => return Ok(Redirect::to("/invoices")),
    };

    // 受注情報を取得
    let order = order_repo::find_received_order(&pool, received_order_id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => return Ok(Redirect::to("/invoices")),
    };

    // 受注明細を取得
    let items = order_repo::list_received_order_items(&pool, received_order_id).await?;

    // 請求書番号を自動採番
    let invoice_id = generate_invoice_id(&pool, form.issue_date).await;

    let subject = form.subject.unwrap_or_else(|| {
        format!("{}分 請求書", order.target_month.format("%Y年%m月"))
    });

    let mut tx = pool.begin().await?;

    // 請求書を作成
    let billing_id = billing_repo::insert_billing_invoice(
        &mut *tx, &invoice_id, order.client_id, received_order_id, form.issue_date, form.due_date,
        &subject, form.notes.as_deref().unwrap_or(""),
    ).await?;

    // 受注明細から請求明細を生成
    for (i, item) in items.iter().enumerate() {
        // 精算計算（settlement::calculate_item_price）
        let amount = crate::domain::services::settlement::calculate_item_price(
            item, item.unit_price, item.effort, item.actual_hours,
        );

        billing_repo::insert_billing_invoice_item(
            &mut *tx, billing_id, item.id, &item.engineer_name, item.unit_price, item.man_month,
            item.actual_hours, item.adjustment, amount, "10%対象", i as i32 + 1,
        ).await?;
    }

    // 受注ステータスを INVOICED に更新
    billing_repo::mark_received_order_invoiced(&mut *tx, received_order_id).await?;

    tx.commit().await?;

    Ok(Redirect::to(&format!("/invoices/{}", billing_id)))
}

/// 請求書番号を自動採番する
/// 形式: INV-YYYYMMDD-001
async fn generate_invoice_id(pool: &PgPool, issue_date: chrono::NaiveDate) -> String {
    let prefix = format!("INV-{}", issue_date.format("%Y%m%d"));

    let max_seq = billing_repo::find_max_invoice_id_with_prefix(pool, &format!("{}%", prefix)).await
        .ok()
        .flatten();

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    format!("{}-{:03}", prefix, next_seq)
}

// ── 編集テンプレート ──

/// 編集フォーム用
#[derive(Debug, serde::Deserialize)]
pub struct EditInvoiceForm {
    pub issue_date: String,
    pub due_date: Option<String>,
    pub subject: Option<String>,
    pub notes: Option<String>,
    #[serde(deserialize_with = "deserialize_string_or_vec")]
    pub item_ids: Vec<String>,
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, String>,
}

/// HTMLフォームのチェックボックスは1つだけ選択時に文字列、複数選択時に配列として送信される。
/// 両方に対応するカスタムデシリアライザ。
fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
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

#[derive(Debug, Deserialize)]
pub struct ApiUpdateInvoiceHeaderForm {
    pub issue_date: String,
    pub due_date: Option<String>,
    pub subject: String,
    pub notes: String,
}

/// PUT /api/v1/invoices/{id} — ヘッダ情報の更新（JSON。明細は対象外・件名/発行日/支払期日/備考のみ）
pub async fn api_update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Json(form): Json<ApiUpdateInvoiceHeaderForm>,
) -> Result<impl IntoResponse, AppError> {
    let issue_date = match chrono::NaiveDate::parse_from_str(&form.issue_date, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "success": false, "error": "発行日の形式が不正です"
            }))).into_response());
        }
    };
    let due_date = form.due_date.as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    billing_repo::update_invoice_header(&pool, id, issue_date, due_date, &form.subject, &form.notes).await?;
    tracing::info!("請求書ヘッダ更新: id={}", id);

    Ok(Json(serde_json::json!({ "success": true })).into_response())
}

/// POST /invoices/{id}/edit — 更新
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Form(form): Form<EditInvoiceForm>,
) -> Result<impl IntoResponse, AppError> {
    let issue_date = chrono::NaiveDate::parse_from_str(&form.issue_date, "%Y-%m-%d").ok();
    let due_date = form.due_date.as_deref()
        .and_then(|s| if s.is_empty() { None } else { chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok() });

    if let Some(issue) = issue_date {
        billing_repo::update_invoice_header(
            &pool, id, issue, due_date, form.subject.as_deref().unwrap_or(""), form.notes.as_deref().unwrap_or(""),
        ).await?;
    }

    // 明細を更新
    for item_id_str in &form.item_ids {
        let item_id: i64 = match item_id_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let product_name = form.fields.get(&format!("product_name_{}", item_id))
            .cloned().unwrap_or_default();
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

        billing_repo::update_invoice_item_fields(
            &pool, item_id, id, &product_name, unit_price, man_month, actual_hours, adjustment, amount,
        ).await?;
    }

    Ok(Redirect::to(&format!("/invoices/{}", id)))
}

/// POST /invoices/{id}/delete — 削除
pub async fn delete(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // 支払通知(パートナー未受諾のみ削除可)と同じ方針: クライアントが受領確認する前なら、
    // 承認済み・送付済みでも削除可とする（送付＝メールが届いただけで、相手方が実際に
    // 見て確定させたとは限らないため。受領確認済みは正式な文書として扱い削除不可）。
    let invoice = billing_repo::find_invoice(&pool, id).await.ok().flatten();
    match invoice {
        Some(ref inv) if inv.client_accepted_at.is_none() => {}
        Some(_) => {
            return Ok((axum::http::StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({
                "success": false, "error": "クライアント受領確認前の請求書のみ削除できます"
            }))).into_response());
        }
        None => {}
    }

    billing_repo::delete_invoice(&pool, id).await?;
    tracing::info!("請求書削除: id={}", id);

    Ok(Redirect::to("/invoices").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct FormWrapper {
        #[serde(deserialize_with = "deserialize_string_or_vec")]
        item_ids: Vec<String>,
    }

    #[test]
    fn deserialize_string_or_vec_accepts_single_string() {
        let parsed: FormWrapper = serde_json::from_str(r#"{"item_ids": "1"}"#).unwrap();
        assert_eq!(parsed.item_ids, vec!["1".to_string()]);
    }

    #[test]
    fn deserialize_string_or_vec_accepts_array() {
        let parsed: FormWrapper = serde_json::from_str(r#"{"item_ids": ["1", "2", "3"]}"#).unwrap();
        assert_eq!(parsed.item_ids, vec!["1".to_string(), "2".to_string(), "3".to_string()]);
    }
}
