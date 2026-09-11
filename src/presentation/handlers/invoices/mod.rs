/// presentation/handlers/invoices/ — 売上請求書 CRUD + 自動生成
///
/// Phase 2: 受注管理 CRUD の一部。
///
/// 機能単位でサブモジュールに分割（2026-07-13、P2-3続き）:
/// - crud: 受注からの自動生成・編集・削除
/// - approval: 承認/差戻し（Admin限定）
/// - mail: 請求書メール送信・プレビュー
/// - pdf: PDFダウンロード
/// - api: SPA用 JSON API（一覧・詳細）
///
/// ## エンドポイント
/// - GET  /invoices             — 一覧
/// - POST /invoices             — 受注から請求書自動生成
/// - GET  /invoices/{id}        — 詳細
/// - POST /api/invoices/{id}/send   — 請求書メール送信
/// - GET  /api/invoices/{id}/pdf    — PDF ダウンロード

mod crud;
mod approval;
mod mail;
pub mod pdf;
mod api;

pub use crud::*;
pub use approval::*;
pub use mail::*;
pub use pdf::*;
pub use api::*;

use sqlx::PgPool;
use crate::infrastructure::repositories::billing_repo;

/// s_company_info テーブルからPDF用データを取得する
async fn get_company_info(pool: &PgPool) -> billing_repo::CompanyInvoiceInfo {
    billing_repo::find_company_invoice_info(pool).await.ok().flatten().unwrap_or_default()
}
