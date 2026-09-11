/// domain/services/pdf_generator.rs — PDF生成サービス
///
/// EDI版(Django/reportlab)のPDF生成コードを流用。
/// Pythonスクリプト(scripts/pdf/pdf_gen.py)をサブプロセスで呼び出し、
/// stdout経由でPDFバイナリを受け取る。
///
/// 対応帳票: 注文書・注文請書・請求書・支払通知書の4種類。
/// フォント: HeiseiMin-W3（reportlab内蔵CIDフォント）

use anyhow::Result;
use chrono::NaiveDate;
use serde::Serialize;
use std::process::Command;
use std::sync::OnceLock;

// ── スクリプトパス ──

/// Pythonスクリプトのパス（Docker内: /app/scripts/pdf/pdf_gen.py）
fn script_path() -> String {
    // Docker内は /app/scripts/pdf/pdf_gen.py
    // ローカルは scripts/pdf/pdf_gen.py
    let docker_path = "/app/scripts/pdf/pdf_gen.py";
    if std::path::Path::new(docker_path).exists() {
        docker_path.to_string()
    } else {
        "scripts/pdf/pdf_gen.py".to_string()
    }
}

/// PDF生成用の python3 を解決する。
///
/// Cursor/Homebrew 環境では PATH先頭の python3 に reportlab が無いことがあるため、
/// `PDF_PYTHON` 指定、または reportlab を import できる候補を優先する。
fn resolve_python() -> &'static str {
    static PYTHON: OnceLock<String> = OnceLock::new();
    PYTHON.get_or_init(|| {
        if let Ok(p) = std::env::var("PDF_PYTHON") {
            if !p.trim().is_empty() {
                return p;
            }
        }
        const CANDIDATES: &[&str] = &[
            "python3",
            "/usr/bin/python3",
            "/Library/Frameworks/Python.framework/Versions/3.9/bin/python3",
            "/Library/Frameworks/Python.framework/Versions/3.13/bin/python3",
            "/usr/local/bin/python3",
            "/opt/homebrew/bin/python3",
        ];
        for candidate in CANDIDATES {
            let ok = Command::new(candidate)
                .args(["-c", "import reportlab"])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if ok {
                return (*candidate).to_string();
            }
        }
        "python3".to_string()
    })
    .as_str()
}

// ── 請求書PDF用データ構造 ──

/// 請求書PDF生成に必要なデータ
#[derive(Serialize)]
pub struct InvoicePdfData {
    pub invoice_id: String,
    #[serde(serialize_with = "serialize_date")]
    pub issue_date: NaiveDate,
    #[serde(serialize_with = "serialize_option_date")]
    pub due_date: Option<NaiveDate>,
    pub client_name: String,
    pub subject: String,
    pub items: Vec<InvoicePdfItem>,
    pub subtotal: i64,
    pub tax_amount: i64,
    pub total: i64,
    pub notes: String,
    // 発行者情報（s_company_info）
    pub company_name: String,
    pub company_postal_code: String,
    pub company_address: String,
    pub company_tel: String,
    pub company_fax: String,
    pub company_representative_title: String,
    pub company_representative_name: String,
    pub registration_number: String,
    // 振込先
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
}

/// 請求明細の1行分
#[derive(Serialize, Default)]
pub struct InvoicePdfItem {
    pub product_name: String,
    pub man_month: String,
    pub unit_price: i64,
    pub amount: i64,
    // 精算情報（超過/控除・稼働時間）。未設定時は amount−unit_price から復元可能
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excess_amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortage_amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduction_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overtime_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_limit_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_limit_hours: Option<f64>,
}

impl InvoicePdfItem {
    /// 請求明細からPDF行を作る。超過/控除は amount − 基本単価 で復元する。
    pub fn from_billing_item(item: &crate::domain::models::billing::BillingItem) -> Self {
        let adjustment = item.amount as i64 - item.unit_price as i64;
        let (excess_amount, shortage_amount) = if adjustment >= 0 {
            (Some(adjustment), Some(0))
        } else {
            (Some(0), Some(-adjustment))
        };
        Self {
            product_name: item.description.clone(),
            // quantity は稼働時間として保存されている（月次一括発行）
            man_month: "1.0".into(),
            unit_price: item.unit_price as i64,
            amount: item.amount as i64,
            excess_amount,
            shortage_amount,
            actual_hours: Some(item.quantity.to_string().parse::<f64>().unwrap_or(0.0)),
            deduction_rate: Some(item.deduction_rate as i64),
            overtime_rate: Some(item.overtime_rate as i64),
            lower_limit_hours: Some(item.lower_limit.to_string().parse::<f64>().unwrap_or(0.0)),
            upper_limit_hours: Some(item.upper_limit.to_string().parse::<f64>().unwrap_or(0.0)),
        }
    }
}

/// 注文書PDF用データ
#[derive(Serialize, Default)]
pub struct PurchaseOrderPdfData {
    pub order_id: String,
    #[serde(serialize_with = "serialize_date")]
    pub order_date: NaiveDate,
    pub partner_name: String,
    pub project_name: String,
    #[serde(serialize_with = "serialize_date")]
    pub work_start: NaiveDate,
    #[serde(serialize_with = "serialize_date")]
    pub work_end: NaiveDate,
    pub items: Vec<OrderPdfItem>,
    pub total: i64,
    pub company_name: String,
    pub company_address: String,
    pub company_tel: String,
    pub representative_name: String,
    // 以下は新規追加（EDI版互換用。Optionalで後方互換性維持）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_postal_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_fax: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partner_postal_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partner_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partner_tel: Option<String>,
    // 作業責任者・作業場所
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_responsible: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workplace: Option<String>,
    // 発注書テンプレート情報
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kou_responsible: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kou_contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otsu_responsible: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otsu_contact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deliverable_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_condition: Option<String>,
}

/// 注文書明細の1行分
#[derive(Serialize, Default)]
pub struct OrderPdfItem {
    pub engineer_name: String,
    pub unit_price: i64,
    pub man_month: String,
    pub amount: i64,
    // EDI版互換: 精算条件
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fee: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduction_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overtime_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_limit_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_limit_hours: Option<f64>,
}

/// 支払通知書PDF用データ
#[derive(Serialize, Default)]
pub struct PaymentNoticePdfData {
    pub notice_id: String,
    #[serde(serialize_with = "serialize_date")]
    pub notice_date: NaiveDate,
    pub partner_name: String,
    pub target_month: String,
    pub items: Vec<PaymentNoticeItem>,
    pub subtotal: i64,
    pub tax_amount: i64,
    pub total: i64,
    #[serde(serialize_with = "serialize_option_date")]
    pub payment_date: Option<NaiveDate>,
    pub company_name: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
    // EDI版互換
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_postal_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_tel: Option<String>,
}

/// 支払通知明細の1行分
#[derive(Serialize, Default)]
pub struct PaymentNoticeItem {
    pub description: String,
    pub unit_price: i64,
    pub quantity: String,
    pub amount: i64,
    // EDI版互換: 精算情報
    #[serde(skip_serializing_if = "Option::is_none")]
    pub engineer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_fee: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excess_amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortage_amount: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduction_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overtime_rate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lower_limit_hours: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upper_limit_hours: Option<f64>,
}

// ── 日付シリアライズヘルパー ──

fn serialize_date<S>(date: &NaiveDate, s: S) -> std::result::Result<S::Ok, S::Error>
where S: serde::Serializer {
    s.serialize_str(&date.format("%Y年%m月%d日").to_string())
}

fn serialize_option_date<S>(date: &Option<NaiveDate>, s: S) -> std::result::Result<S::Ok, S::Error>
where S: serde::Serializer {
    match date {
        Some(d) => s.serialize_str(&d.format("%Y年%m月%d日").to_string()),
        None => s.serialize_none(),
    }
}

// ── PDF生成エンジン ──

pub struct PdfGenerator;

impl PdfGenerator {
    pub fn new() -> Self { Self }

    /// Python呼び出し共通処理
    fn call_python(pdf_type: &str, json_data: &str) -> Result<Vec<u8>> {
        let script = script_path();
        let python = resolve_python();
        let output = Command::new(python)
            .args([&script, "--type", pdf_type, "--json", json_data])
            .output()
            .map_err(|e| anyhow::anyhow!(
                "Python実行エラー: {} (PDF_PYTHON または reportlab 入り python3 を設定してください)",
                e
            ))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("PDF生成失敗({} via {}): {}", pdf_type, python, stderr);
        }

        if output.stdout.len() < 10 {
            anyhow::bail!("PDF出力が空です({})", pdf_type);
        }

        Ok(output.stdout)
    }

    // ================================================================
    // 請求書PDF
    // ================================================================

    pub fn generate_invoice_pdf(&self, data: &InvoicePdfData) -> Result<Vec<u8>> {
        // Python `invoice` テンプレートは歴史的に「代理請求書」向き:
        //   company = 宛先（御中）、partner = 発行者
        // クライアント向け自社請求では宛先=クライアント・発行者=自社にマッピングする。
        // 振込先(bank)は常に自社口座（クライアント→自社への入金）のまま。
        let representative = {
            let title = data.company_representative_title.trim();
            let name = data.company_representative_name.trim();
            if title.is_empty() {
                name.to_string()
            } else if name.is_empty() {
                title.to_string()
            } else {
                format!("{} {}", title, name)
            }
        };
        let python_data = serde_json::json!({
            "invoice_id": data.invoice_id,
            "issue_date": data.issue_date.format("%Y年%m月%d日").to_string(),
            "subtotal": data.subtotal,
            "tax_amount": data.tax_amount,
            "total": data.total,
            "project_name": data.subject,
            "subject": data.subject,
            "payment_deadline": data.due_date
                .map(|d| d.format("%Y年%m月%d日").to_string())
                .unwrap_or_default(),
            "company": {
                "name": data.client_name,
            },
            "partner": {
                "name": data.company_name,
                "postal_code": data.company_postal_code,
                "address": data.company_address,
                "tel": data.company_tel,
                "fax": data.company_fax,
                "representative": representative,
            },
            "registration_number": data.registration_number,
            "show_stamp": true,
            "items": data.items.iter().map(|item| {
                // 増/超過・減/控除は必ず amount − 基本単価 と一致させる
                // （時間×超過単価の再計算は精算幅と食い違う場合があるため使わない）
                let adjustment = item.amount - item.unit_price;
                let excess = if adjustment > 0 { adjustment } else { 0 };
                let shortage = if adjustment < 0 { -adjustment } else { 0 };
                serde_json::json!({
                    "engineer_name": item.product_name,
                    "base_fee": item.unit_price,
                    "amount": item.amount,
                    "excess_amount": excess,
                    "shortage_amount": shortage,
                    "actual_hours": item.actual_hours.unwrap_or(0.0),
                    "effort": item.man_month.parse::<f64>().unwrap_or(1.0),
                    "deduction_rate": item.deduction_rate.unwrap_or(0),
                    "overtime_rate": item.overtime_rate.unwrap_or(0),
                    "lower_limit_hours": item.lower_limit_hours.unwrap_or(0.0),
                    "upper_limit_hours": item.upper_limit_hours.unwrap_or(0.0),
                })
            }).collect::<Vec<_>>(),
            "bank": {
                "bank_name": data.bank_name,
                "bank_branch": data.bank_branch,
                "account_type": data.account_type,
                "account_number": data.account_number,
                "account_name": data.account_name,
            },
        });
        Self::call_python("invoice", &python_data.to_string())
    }

    // ================================================================
    // 注文書PDF
    // ================================================================

    pub fn generate_purchase_order_pdf(&self, data: &PurchaseOrderPdfData) -> Result<Vec<u8>> {
        let python_data = serde_json::json!({
            "order_id": data.order_id,
            "order_date": data.order_date.format("%Y年%m月%d日").to_string(),
            "project_name": data.project_name,
            "work_start": data.work_start.format("%Y年%m月%d日").to_string(),
            "work_end": data.work_end.format("%Y年%m月%d日").to_string(),
            "company": {
                "name": data.company_name,
                "postal_code": data.company_postal_code.as_deref().unwrap_or(""),
                "address": data.company_address,
                "tel": data.company_tel,
                "fax": data.company_fax.as_deref().unwrap_or(""),
                "responsible_person": data.representative_name,
            },
            "partner": {
                "name": data.partner_name,
                "postal_code": data.partner_postal_code.as_deref().unwrap_or(""),
                "address": data.partner_address.as_deref().unwrap_or(""),
                "tel": data.partner_tel.as_deref().unwrap_or(""),
            },
            "items": data.items.iter().map(|item| serde_json::json!({
                "engineer_name": item.engineer_name,
                "base_fee": item.base_fee.unwrap_or(item.unit_price),
                "deduction_rate": item.deduction_rate.unwrap_or(0),
                "overtime_rate": item.overtime_rate.unwrap_or(0),
                "lower_limit_hours": item.lower_limit_hours.unwrap_or(0.0),
                "upper_limit_hours": item.upper_limit_hours.unwrap_or(0.0),
            })).collect::<Vec<_>>(),
            "work_responsible": data.work_responsible.as_deref().unwrap_or(""),
            "workplace": data.workplace.as_deref().unwrap_or(""),
            "kou_responsible": data.kou_responsible.as_deref().unwrap_or(""),
            "kou_contact": data.kou_contact.as_deref().unwrap_or(""),
            "otsu_responsible": data.otsu_responsible.as_deref().unwrap_or(""),
            "otsu_contact": data.otsu_contact.as_deref().unwrap_or(""),
            "deliverable_text": data.deliverable_text.as_deref().unwrap_or("作業報告書"),
            "payment_condition": data.payment_condition.as_deref().unwrap_or("月末締め翌月末払い"),
        });
        Self::call_python("order", &python_data.to_string())
    }

    // ================================================================
    // 注文書（案）PDF — 注文書と同じ内容
    // ================================================================

    pub fn generate_draft_order_pdf(&self, data: &PurchaseOrderPdfData) -> Result<Vec<u8>> {
        // 現時点では注文書と同じPDFを生成（将来ウォーターマーク追加可能）
        self.generate_purchase_order_pdf(data)
    }

    // ================================================================
    // 注文請書PDF
    // ================================================================

    pub fn generate_acceptance_pdf(&self, data: &PurchaseOrderPdfData) -> Result<Vec<u8>> {
        let python_data = serde_json::json!({
            "order_id": data.order_id,
            "order_date": data.order_date.format("%Y年%m月%d日").to_string(),
            "project_name": data.project_name,
            "work_start": data.work_start.format("%Y年%m月%d日").to_string(),
            "work_end": data.work_end.format("%Y年%m月%d日").to_string(),
            "company": {
                "name": data.company_name,
                "postal_code": data.company_postal_code.as_deref().unwrap_or(""),
                "address": data.company_address,
                "tel": data.company_tel,
                "fax": data.company_fax.as_deref().unwrap_or(""),
                "responsible_person": data.representative_name,
            },
            "partner": {
                "name": data.partner_name,
                "postal_code": data.partner_postal_code.as_deref().unwrap_or(""),
                "address": data.partner_address.as_deref().unwrap_or(""),
                "tel": data.partner_tel.as_deref().unwrap_or(""),
            },
            "items": data.items.iter().map(|item| serde_json::json!({
                "engineer_name": item.engineer_name,
                "base_fee": item.base_fee.unwrap_or(item.unit_price),
                "deduction_rate": item.deduction_rate.unwrap_or(0),
                "overtime_rate": item.overtime_rate.unwrap_or(0),
                "lower_limit_hours": item.lower_limit_hours.unwrap_or(0.0),
                "upper_limit_hours": item.upper_limit_hours.unwrap_or(0.0),
            })).collect::<Vec<_>>(),
            "work_responsible": data.work_responsible.as_deref().unwrap_or(""),
            "workplace": data.workplace.as_deref().unwrap_or(""),
            "kou_responsible": data.kou_responsible.as_deref().unwrap_or(""),
            "kou_contact": data.kou_contact.as_deref().unwrap_or(""),
            "otsu_responsible": data.otsu_responsible.as_deref().unwrap_or(""),
            "otsu_contact": data.otsu_contact.as_deref().unwrap_or(""),
            "deliverable_text": data.deliverable_text.as_deref().unwrap_or("作業報告書"),
            "payment_condition": data.payment_condition.as_deref().unwrap_or("月末締め翌月末払い"),
        });
        Self::call_python("acceptance", &python_data.to_string())
    }

    // ================================================================
    // 支払通知書PDF
    // ================================================================

    pub fn generate_payment_notice_pdf(&self, data: &PaymentNoticePdfData) -> Result<Vec<u8>> {
        let python_data = serde_json::json!({
            "notice_id": data.notice_id,
            "notice_date": data.notice_date.format("%Y年%m月%d日").to_string(),
            "target_month": data.target_month,
            "subtotal": data.subtotal,
            "tax_amount": data.tax_amount,
            "total": data.total,
            "project_name": data.project_name.as_deref().unwrap_or(""),
            "payment_date": data.payment_date
                .map(|d| d.format("%Y年%m月%d日").to_string())
                .unwrap_or_else(|| "ご登録支払サイト日".to_string()),
            "company": {
                "name": data.company_name,
                "postal_code": data.company_postal_code.as_deref().unwrap_or(""),
                "address": data.company_address.as_deref().unwrap_or(""),
                "tel": data.company_tel.as_deref().unwrap_or(""),
            },
            "partner": {
                "name": data.partner_name,
            },
            "items": data.items.iter().map(|item| serde_json::json!({
                "engineer_name": item.engineer_name.as_deref().unwrap_or(&item.description),
                "base_fee": item.base_fee.unwrap_or(item.unit_price),
                "amount": item.amount,
                "excess_amount": item.excess_amount.unwrap_or(0),
                "shortage_amount": item.shortage_amount.unwrap_or(0),
                "actual_hours": item.actual_hours.unwrap_or(0.0),
                "effort": item.quantity.parse::<f64>().unwrap_or(1.0),
                "deduction_rate": item.deduction_rate.unwrap_or(0),
                "overtime_rate": item.overtime_rate.unwrap_or(0),
                "lower_limit_hours": item.lower_limit_hours.unwrap_or(0.0),
                "upper_limit_hours": item.upper_limit_hours.unwrap_or(0.0),
            })).collect::<Vec<_>>(),
        });
        Self::call_python("notice", &python_data.to_string())
    }

    // ================================================================
    // パートナー代理請求書PDF（自社がパートナー名義で作成する請求書）
    // ================================================================

    /// 支払通知と同じ明細から、パートナー名義の請求書PDFを生成する。
    /// Python `invoice` テンプレートは宛先=company（自社）、発行者=partner の前提。
    pub fn generate_partner_invoice_pdf(
        &self,
        data: &PaymentNoticePdfData,
        partner: &PartnerInvoiceIssuer,
    ) -> Result<Vec<u8>> {
        let python_data = serde_json::json!({
            "invoice_id": data.notice_id,
            "issue_date": data.notice_date.format("%Y年%m月%d日").to_string(),
            "subtotal": data.subtotal,
            "tax_amount": data.tax_amount,
            "total": data.total,
            "project_name": data.project_name.as_deref().unwrap_or(""),
            "subject": data.project_name.as_deref().unwrap_or(""),
            "payment_deadline": data.payment_date
                .map(|d| d.format("%Y年%m月%d日").to_string())
                .unwrap_or_default(),
            "company": {
                "name": data.company_name,
                "postal_code": data.company_postal_code.as_deref().unwrap_or(""),
                "address": data.company_address.as_deref().unwrap_or(""),
                "tel": data.company_tel.as_deref().unwrap_or(""),
            },
            "partner": {
                "name": partner.name,
                "postal_code": partner.postal_code,
                "address": partner.address,
                "tel": partner.tel,
                "representative": partner.representative,
            },
            "registration_number": partner.registration_no,
            "items": data.items.iter().map(|item| serde_json::json!({
                "engineer_name": item.engineer_name.as_deref().unwrap_or(&item.description),
                "base_fee": item.base_fee.unwrap_or(item.unit_price),
                "amount": item.amount,
                "excess_amount": item.excess_amount.unwrap_or(0),
                "shortage_amount": item.shortage_amount.unwrap_or(0),
                "actual_hours": item.actual_hours.unwrap_or(0.0),
                "effort": item.quantity.parse::<f64>().unwrap_or(1.0),
                "deduction_rate": item.deduction_rate.unwrap_or(0),
                "overtime_rate": item.overtime_rate.unwrap_or(0),
                "lower_limit_hours": item.lower_limit_hours.unwrap_or(0.0),
                "upper_limit_hours": item.upper_limit_hours.unwrap_or(0.0),
            })).collect::<Vec<_>>(),
            "bank": {
                "bank_name": partner.bank_name,
                "bank_branch": partner.bank_branch,
                "account_type": partner.account_type,
                "account_number": partner.account_number,
                "account_name": partner.account_name,
            },
        });
        Self::call_python("invoice", &python_data.to_string())
    }
}

/// パートナー代理請求書の発行者（パートナー）情報
#[derive(Debug, Clone, Default)]
pub struct PartnerInvoiceIssuer {
    pub name: String,
    pub postal_code: String,
    pub address: String,
    pub tel: String,
    pub representative: String,
    pub registration_no: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
}

// ── ユーティリティ（残存: format_number はテンプレートで使用可能） ──

/// 数値をカンマ区切りフォーマット
pub fn format_number(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_number(-n));
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(c);
    }
    result.chars().rev().collect()
}

// ── テスト ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(100), "100");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
        assert_eq!(format_number(-500), "-500");
        assert_eq!(format_number(-1234567), "-1,234,567");
    }

    #[test]
    fn test_generate_purchase_order_pdf() {
        let gen = PdfGenerator::new();
        let data = PurchaseOrderPdfData {
            order_id: "PO-202607-001".into(),
            order_date: NaiveDate::from_ymd_opt(2026, 6, 27).unwrap(),
            partner_name: "テストパートナーA社".into(),
            project_name: "テストプロジェクトC".into(),
            work_start: NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
            work_end: NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
            items: vec![OrderPdfItem {
                engineer_name: "テスト太郎".into(),
                unit_price: 850000,
                man_month: "1.00".into(),
                amount: 850000,
                base_fee: Some(850000),
                deduction_rate: Some(6070),
                overtime_rate: Some(4720),
                lower_limit_hours: Some(140.0),
                upper_limit_hours: Some(180.0),
            }],
            total: 850000,
            company_name: "テスト株式会社".into(),
            company_address: "東京都千代田区".into(),
            company_tel: "03-0000-0000".into(),
            representative_name: "代表太郎".into(),
            company_postal_code: Some("100-0001".into()),
            company_fax: None,
            partner_postal_code: None,
            partner_address: None,
            partner_tel: None,
            work_responsible: None,
            workplace: None,
            kou_responsible: None,
            kou_contact: None,
            otsu_responsible: None,
            otsu_contact: None,
            deliverable_text: None,
            payment_condition: None,
        };
        let result = gen.generate_purchase_order_pdf(&data);
        assert!(result.is_ok(), "注文書PDF生成失敗: {:?}", result.err());
        let pdf = result.unwrap();
        assert!(pdf.len() > 100, "PDFサイズ不正: {} bytes", pdf.len());
        assert_eq!(&pdf[0..5], b"%PDF-", "PDFヘッダー不正");
    }
}

