/// presentation/handlers/webapi/dto.rs — WebAPI レスポンス用DTO

use chrono::{NaiveDate, DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// クライアント向け請求書APIレスポンス
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiInvoiceResponse {
    pub invoice_id: String,
    pub issue_date: NaiveDate,
    pub due_date: Option<NaiveDate>,
    pub seller: ApiParty,
    pub buyer: ApiParty,
    pub lines: Vec<ApiInvoiceLine>,
    pub tax_summary: Vec<ApiTaxSubtotal>,
    pub payable_amount: i32,
    pub status: String,
    pub client_accepted_at: Option<DateTime<Utc>>,
}

/// 取引相手情報
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiParty {
    pub name: String,
    pub postal_code: Option<String>,
    pub address: Option<String>,
    pub contact: Option<String>,
}

/// 請求書行項目
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiInvoiceLine {
    pub description: String,
    pub quantity: String,
    pub unit_price: i32,
    pub amount: i32,
}

/// 税計算サマリー（JP拡張）
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ApiTaxSubtotal {
    pub tax_rate: String,
    pub taxable_amount: i32,
    pub tax_amount: i32,
}

/// 発注データ送信リクエスト
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ApiOrderSubmitRequest {
    pub order_date: NaiveDate,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
    pub items: Vec<ApiOrderItem>,
    pub remarks: Option<String>,
}

/// 発注行項目
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ApiOrderItem {
    pub engineer_id: String,
    pub description: String,
    pub unit_price: i32,
    pub quantity: i32,
}

/// 発注登録レスポンス
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiOrderSubmitResponse {
    pub received_order_id: i64,
    pub received_order_no: String,
}

/// APIエラーレスポンス
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiErrorResponse {
    pub error: String,
    pub message: Option<String>,
}

/// JpPintInvoice → ApiInvoiceResponse 変換（WebAPI用）
pub fn map_jp_pint_to_api(
    jp: &crate::domain::services::jp_pint_mapper::JpPintInvoice,
    billing_status: &str,
    client_accepted_at: Option<DateTime<Utc>>,
) -> ApiInvoiceResponse {
    ApiInvoiceResponse {
        invoice_id: jp.invoice_id.clone(),
        issue_date: jp.issue_date,
        due_date: jp.due_date,
        seller: ApiParty {
            name: jp.seller.registration_name.clone(),
            postal_code: Some(jp.seller.postal_address.postal_code.clone()),
            address: Some(jp.seller.postal_address.address_line.clone()),
            contact: None,
        },
        buyer: ApiParty {
            name: jp.buyer.registration_name.clone(),
            postal_code: Some(jp.buyer.postal_address.postal_code.clone()),
            address: Some(jp.buyer.postal_address.address_line.clone()),
            contact: None,
        },
        lines: jp
            .lines
            .iter()
            .map(|line| ApiInvoiceLine {
                description: line.item_name.clone(),
                quantity: line.quantity.to_string(),
                unit_price: line.unit_price,
                amount: line.line_amount,
            })
            .collect(),
        tax_summary: jp
            .tax_subtotals
            .iter()
            .map(|tax| ApiTaxSubtotal {
                tax_rate: tax.tax_rate.to_string(),
                taxable_amount: tax.taxable_amount,
                tax_amount: tax.tax_amount,
            })
            .collect(),
        payable_amount: jp.payable_amount,
        status: billing_status.to_string(),
        client_accepted_at,
    }
}
