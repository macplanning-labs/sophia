/// orders/legacy.rs — SSRフォーム時代のハンドラ（create/rollforward/publish/update/delete等）
///
/// 一部は`-legacy`サフィックス付きルートとして現役、一部（create/update）はルーティングされていない
/// dead code（P2-3安定化計画の未決事項リスト参照）。

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    Form,
};
use sqlx::PgPool;

use crate::infrastructure::repositories::order_repo::{self, ContractForOrder};
use crate::presentation::api_response::AppError;

use super::pdf::build_purchase_order_pdf_data;
use super::{generate_order_id, StatusForm};

/// POST /orders — 作成（未ルーティングのdead code。ルーティング済みの相当機能は`orders/api.rs::api_create`）
pub async fn create(
    State(pool): State<PgPool>,
    Form(form): Form<CreateOrderForm>,
) -> Result<impl IntoResponse, AppError> {
    let work_start = match chrono::NaiveDate::parse_from_str(&form.work_start, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(Redirect::to("/orders/new")),
    };
    let work_end = match chrono::NaiveDate::parse_from_str(&form.work_end, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return Ok(Redirect::to("/orders/new")),
    };

    let contract_ids: Vec<i64> = form.contract_ids.iter()
        .filter_map(|s| s.parse::<i64>().ok())
        .collect();

    if contract_ids.is_empty() {
        return Ok(Redirect::to("/orders/new"));
    }

    // 選択された契約情報を取得
    let contracts = order_repo::find_contracts_for_order(&pool, &contract_ids).await?;

    // パートナーID別にグルーピング
    use std::collections::HashMap;
    let mut groups: HashMap<String, Vec<&ContractForOrder>> = HashMap::new();
    for c in &contracts {
        groups.entry(c.partner_id.clone()).or_default().push(c);
    }

    let mut tx = pool.begin().await?;

    for (partner_id, group) in &groups {
        let first = group[0];
        // 同一条件のDRAFT発注書が既に存在する場合はスキップ（二重作成防止）
        let draft_exists = order_repo::draft_order_exists(
            &mut *tx, partner_id, &first.project_id, first.id, work_start, work_end,
        ).await?;

        if draft_exists {
            tracing::warn!("Skipping duplicate DRAFT PO for partner={} project={}", partner_id, first.project_id);
            continue;
        }

        // 発注書番号採番
        let order_id = generate_order_id(&pool, work_start).await;

        order_repo::insert_purchase_order(
            &mut *tx, &order_id, partner_id, &first.project_id, first.id, work_start, work_end,
        ).await?;

        // 明細を作成
        for c in group {
            let price = (c.base_rate as f64 * rust_decimal::prelude::ToPrimitive::to_f64(&c.effort).unwrap_or(1.0)) as i32;
            order_repo::insert_purchase_order_item(
                &mut *tx, &order_id, c.id, c.base_rate, c.effort, &c.settlement_type,
                c.lower_limit_hours, c.upper_limit_hours, c.fixed_hours,
                c.deduction_rate, c.overtime_rate, price,
            ).await?;
        }
    }

    tx.commit().await?;

    Ok(Redirect::to("/orders"))
}

/// POST /orders/{id}/rollforward — 翌月ロールフォワード（未ルーティングのdead code。相当機能は`orders/api.rs::api_rollforward`）
pub async fn rollforward(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // 元の注文書を取得
    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    match order {
        Some(o) => {
            let new_start = o.work_start + chrono::Months::new(1);
            let new_end = o.work_end + chrono::Months::new(1);
            let new_order_id = generate_order_id(&pool, new_start).await;

            let mut tx = pool.begin().await?;

            // 同一条件のDRAFT発注書が既に存在するかチェック（二重作成防止）
            let draft_exists = order_repo::draft_order_exists(
                &mut *tx, &o.partner_id, &o.project_id, o.partner_contract_id.unwrap_or(0), new_start, new_end,
            ).await?;

            if draft_exists {
                tracing::warn!("Rollforward skipped: DRAFT PO already exists for partner={} project={} period={}-{}", o.partner_id, o.project_id, new_start, new_end);
                return Ok(Redirect::to("/orders"));
            }

            order_repo::insert_purchase_order_full(
                &mut *tx, &new_order_id, &o.partner_id, &o.project_id,
                o.partner_contract_id.unwrap_or(0), new_start, new_end,
                o.workplace_id, &o.deliverable_text, &o.payment_condition,
                &o.contract_items, &o.remarks,
            ).await?;

            // 明細をコピー
            order_repo::copy_order_items(&mut tx, &id, &new_order_id).await?;

            tx.commit().await?;
            Ok(Redirect::to(&format!("/orders/{}", new_order_id)))
        }
        None => Ok(Redirect::to("/orders")),
    }
}

/// POST /orders/{id}/publish — 注文書PDF添付メール送付（未ルーティングのdead code。相当機能は`orders/api.rs::api_publish`）
pub async fn publish(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::services::pdf_generator::{PdfGenerator, PurchaseOrderPdfData, OrderPdfItem};
    use crate::domain::services::email_service::EmailService;

    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => return Ok(Redirect::to("/orders")),
    };

    // パートナー情報
    let partner_info = order_repo::find_partner_name_email(&pool, &order.partner_id).await.ok().flatten();
    let (partner_name, partner_email) = partner_info.unwrap_or_default();

    let project_name = order_repo::find_project_name(&pool, &order.project_id).await?
        .unwrap_or_default();

    let items = order_repo::list_order_items_ordered(&pool, &id).await?;

    // CompanyInfo 取得
    let company = order_repo::find_company_info(&pool).await.ok().flatten();
    let (company_name, company_addr, company_tel, rep_name) = company.unwrap_or_default();

    let total: i64 = items.iter().map(|i| i.amount as i64).sum();

    // PDF生成
    let pdf_data = PurchaseOrderPdfData {
        order_id: order.order_id.clone(),
        order_date: order.order_date,
        partner_name: partner_name.clone(),
        project_name: project_name.clone(),
        work_start: order.work_start,
        work_end: order.work_end,
        items: items.iter().map(|item| OrderPdfItem {
            engineer_name: format!("契約#{}", item.partner_contract_id),
            unit_price: item.base_fee as i64,
            man_month: item.effort.to_string(),
            amount: item.amount as i64,
            ..Default::default()
        }).collect(),
        total,
        company_name,
        company_address: company_addr,
        company_tel,
        representative_name: rep_name,
        ..Default::default()
    };

    let gen = PdfGenerator::new();
    let _pdf_bytes = match gen.generate_purchase_order_pdf(&pdf_data) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("発注書PDF生成エラー: {}", e);
            return Ok(Redirect::to(&format!("/orders/{}", id)));
        }
    };

    // メール送信（テンプレート方式: EDIと同じ文面をDB管理）
    // 送信に失敗した場合はステータスをSENTにせず処理を打ち切る（未送信のまま送付済み扱いになるのを防ぐ）
    if !partner_email.is_empty() {
        let base_url = std::env::var("BASE_URL").unwrap_or_default();
        let token_url = format!("{}/token/{}", base_url, order.uuid);
        let month = order.work_start.format("%Y年%m月").to_string();
        let expiry_days = crate::domain::services::settlement_dashboard::get_token_expiry_days(&pool).await;
        let ctx = crate::domain::services::email_service::compose_order_publish_email(
            &partner_name, &order.order_id, &month, &token_url, expiry_days,
        );
        let email_svc = EmailService::new(pool.clone());
        email_svc.send_by_template("order_send", &partner_email, None, &ctx).await?;
    }

    // ステータスを SENT に更新
    order_repo::mark_purchase_order_sent(&pool, &id).await?;

    Ok(Redirect::to(&format!("/orders/{}", id)))
}

/// GET /orders/{id}/pdf — 注文書PDFダウンロード
pub async fn download_pdf(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("発注書が見つかりません"))
                .expect("Response builder should not fail");
        }
    };

    let pdf_data = match build_purchase_order_pdf_data(&pool, &order).await {
        Ok(d) => d,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!("PDFデータ構築エラー: {}", e)))
                .expect("Response builder should not fail");
        }
    };

    let gen = PdfGenerator::new();
    match gen.generate_purchase_order_pdf(&pdf_data) {
        Ok(pdf_bytes) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/pdf")
                .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"order_{}.pdf\"", order.order_id))
                .body(Body::from(pdf_bytes))
                .expect("Response builder should not fail")
        }
        Err(e) => {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!("PDF生成エラー: {}", e)))
                .expect("Response builder should not fail")
        }
    }
}

/// GET /orders/{id}/acceptance-pdf — 注文請書PDFダウンロード
pub async fn download_acceptance_pdf(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use axum::response::Response;
    use axum::body::Body;
    use axum::http::{header, StatusCode};
    use crate::domain::services::pdf_generator::PdfGenerator;

    let order = order_repo::find_purchase_order(&pool, &id).await.ok().flatten();

    let order = match order {
        Some(o) => o,
        None => {
            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("発注書が見つかりません"))
                .expect("Response builder should not fail");
        }
    };

    let pdf_data = match build_purchase_order_pdf_data(&pool, &order).await {
        Ok(d) => d,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!("PDFデータ構築エラー: {}", e)))
                .expect("Response builder should not fail");
        }
    };

    let gen = PdfGenerator::new();
    match gen.generate_acceptance_pdf(&pdf_data) {
        Ok(pdf_bytes) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/pdf")
                .header(header::CONTENT_DISPOSITION, format!("inline; filename=\"acceptance_{}.pdf\"", order.order_id))
                .body(Body::from(pdf_bytes))
                .expect("Response builder should not fail")
        }
        Err(e) => {
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(format!("注文請書PDF生成エラー: {}", e)))
                .expect("Response builder should not fail")
        }
    }
}

pub async fn update_status(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Form(form): Form<StatusForm>,
) -> Result<impl IntoResponse, AppError> {
    let valid = ["DRAFT", "SENT", "ACCEPTED", "REPORT_RECEIVED",
                 "NOTICE_CREATED", "NOTICE_CONFIRMED", "PAID"];
    if !valid.contains(&form.status.as_str()) {
        return Ok(Redirect::to(&format!("/orders/{}", id)));
    }

    order_repo::update_purchase_order_status(&pool, &id, &form.status).await?;

    Ok(Redirect::to(&format!("/orders/{}", id)))
}

// ── フォーム ──

#[derive(Debug, serde::Deserialize)]
pub struct CreateOrderForm {
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub contract_ids: Vec<String>,
    pub work_start: String,
    pub work_end: String,
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
            let mut vec = Vec::new();
            while let Some(val) = seq.next_element::<String>()? {
                vec.push(val);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

// ── 編集テンプレート ──


/// 編集フォーム用
#[derive(Debug, serde::Deserialize)]
pub struct EditOrderForm {
    pub work_start: String,
    pub work_end: String,
    #[serde(default)]
    pub item_ids: OneOrMany,
    pub work_location: Option<String>,
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, String>,
}

/// HTMLフォームが単一値を文字列、複数値を配列で送るため両方受け付ける
#[derive(Debug)]
pub struct OneOrMany(pub Vec<String>);
impl Default for OneOrMany {
    fn default() -> Self { Self(vec![]) }
}
impl<'de> serde::Deserialize<'de> for OneOrMany {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de;
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = OneOrMany;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "a string or sequence of strings")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<OneOrMany, E> {
                if v.is_empty() { Ok(OneOrMany(vec![])) }
                else { Ok(OneOrMany(vec![v.to_string()])) }
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<OneOrMany, A::Error> {
                let mut v = vec![];
                while let Some(s) = seq.next_element::<String>()? { v.push(s); }
                Ok(OneOrMany(v))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// POST /orders/{id}/edit — 更新（DRAFTのみ、未ルーティングのdead code。相当機能は`orders/api.rs::api_update`）
pub async fn update(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Form(form): Form<EditOrderForm>,
) -> Result<impl IntoResponse, AppError> {
    // DRAFTチェック
    let status = order_repo::find_purchase_order_status(&pool, &id).await.ok().flatten();

    if status.as_deref() != Some("DRAFT") {
        return Ok(Redirect::to(&format!("/orders/{}", id)));
    }

    let work_start = chrono::NaiveDate::parse_from_str(&form.work_start, "%Y-%m-%d").ok();
    let work_end = chrono::NaiveDate::parse_from_str(&form.work_end, "%Y-%m-%d").ok();

    if let (Some(ws), Some(we)) = (work_start, work_end) {
        let wl = form.work_location.as_deref().unwrap_or("");
        order_repo::update_purchase_order_work(&pool, &id, ws, we, wl, None).await?;
    }

    // 明細を更新
    for item_id_str in &form.item_ids.0 {
        let item_id: i64 = match item_id_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        let base_fee: i32 = form.fields.get(&format!("base_fee_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or(0);
        let effort: rust_decimal::Decimal = form.fields.get(&format!("effort_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or_default();
        let settlement_type = form.fields.get(&format!("settlement_type_{}", item_id))
            .cloned().unwrap_or_default();
        let lower_limit: rust_decimal::Decimal = form.fields.get(&format!("lower_limit_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or_default();
        let upper_limit: rust_decimal::Decimal = form.fields.get(&format!("upper_limit_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or_default();
        let deduction_rate: i32 = form.fields.get(&format!("deduction_rate_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or(0);
        let overtime_rate: i32 = form.fields.get(&format!("overtime_rate_{}", item_id))
            .and_then(|s| s.parse().ok()).unwrap_or(0);

        // 金額を再計算
        let price = (base_fee as f64 * rust_decimal::prelude::ToPrimitive::to_f64(&effort).unwrap_or(1.0)) as i32;

        order_repo::update_purchase_order_item_fields(
            &pool, item_id, &id, base_fee, effort, &settlement_type,
            lower_limit, upper_limit, deduction_rate, overtime_rate, price,
        ).await?;
    }

    Ok(Redirect::to(&format!("/orders/{}", id)))
}

/// POST /orders/{id}/delete — 削除（DRAFTのみ）
pub async fn delete(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let status = order_repo::find_purchase_order_status(&pool, &id).await.ok().flatten();

    if status.as_deref() == Some("DRAFT") {
        order_repo::delete_purchase_order(&pool, &id).await?;
        tracing::info!("発注書削除: {}", id);
    }

    Ok(Redirect::to("/orders"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize)]
    struct FormWrapper {
        #[serde(default, deserialize_with = "deserialize_string_or_vec")]
        contract_ids: Vec<String>,
    }

    #[test]
    fn deserialize_string_or_vec_accepts_single_string() {
        let parsed: FormWrapper = serde_json::from_str(r#"{"contract_ids": "42"}"#).unwrap();
        assert_eq!(parsed.contract_ids, vec!["42".to_string()]);
    }

    #[test]
    fn deserialize_string_or_vec_accepts_array() {
        let parsed: FormWrapper = serde_json::from_str(r#"{"contract_ids": ["1", "2"]}"#).unwrap();
        assert_eq!(parsed.contract_ids, vec!["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn deserialize_string_or_vec_defaults_to_empty_when_missing() {
        let parsed: FormWrapper = serde_json::from_str(r#"{}"#).unwrap();
        assert!(parsed.contract_ids.is_empty());
    }
}
