//! custom — 当社固有のカスタマイズ（取引先ごとの連携）への差し込み口
//!
//! 汎用の部分（メール取込・書類の受付・取引先マスタ）は、取引先ごとの連携を、このファイルの関数・定数だけで使う。
//! カスタマイズ（Cargo の feature `custom`）が無いビルド（公開版）では、`registry_none.rs`（何もしない版）が使われる。
//! このファイルと registry_none.rs には、取引先の名前を書かない（公開する）。取引先の名前は registry.rs と各取引先のフォルダにだけ書く。
//! 取引先の社名・ドメイン・取引先のシステムの使い方は、各カスタマイズのフォルダの中にだけ書く。
//! 他の取引先の連携は、依頼が来たときに足す（先回りの汎用化はしない）。

use chrono::NaiveDate;

use crate::infrastructure::attachment_parsers::order_pdf_parser::ParsedOrder;
use crate::infrastructure::mail_pipeline::phase3_parse_register::NormalizedOrder;

// 取引先ごとのカスタマイズの組み込み（どの取引先の処理を、どの関数につなぐか）。
// registry.rs は取引先の名前を含むため公開しない。公開版（feature `custom` 無し）では registry_none.rs が使われる。
#[cfg(feature = "custom")]
#[path = "registry.rs"]
mod registry;
#[cfg(not(feature = "custom"))]
#[path = "registry_none.rs"]
mod registry;

pub use registry::*;

/// 取引先 EDI の通知メールの差出人と一致した取引先
#[derive(Debug, Clone)]
pub struct EdiNoticeSender {
    pub client_id: i64,
    pub client_name: String,
}

/// 取引先 EDI の通知メールを処理した結果（メール取込 Phase2）
#[derive(Debug, Default)]
pub struct EdiFetchSummary {
    pub notices: usize,
    pub new_orders: usize,
    pub new_invoices: usize,
    pub errors: Vec<String>,
}

/// 取引先 EDI から取得した書類を、汎用の「書類の受付」（Phase3）に渡す形
pub enum EdiDocument {
    /// 注文書 → 受注として登録する
    Order(NormalizedOrder),
    /// 支払通知 → 自社の請求書と突合する
    PaymentNotice { client_id: i64, target_month: NaiveDate, invoice_no: String, total: i64 },
}

/// 取引先ごとの注文書 PDF の様式（汎用の注文書 PDF の読み取りが、汎用の形式より先に判定する）
pub struct OrderPdfFormat {
    /// 形式の名前（読み取り結果の format に入る）
    pub name: &'static str,
    pub detect: fn(&str) -> bool,
    pub parse: fn(&str, &mut ParsedOrder),
}
