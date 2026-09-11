/// infrastructure/edi_oasis_client.rs — EDI-OASIS REST API クライアント
///
/// イービジネス社の EDI-OASIS システムに JWT 認証で接続し、
/// 注文書・請求書データを JSON/PDFで取得する。
///
/// メール自動取込パイプライン(infrastructure::mail_pipeline)のPhase2からは、
/// メール本文のURL抽出ではなく fetch_orders/fetch_invoices による年月ポーリングで
/// 呼び出される（メールの有無に依存しない取得経路）。
///
/// ## API エンドポイント（Python版から判明）
/// - POST /v1/login            → JWT 認証
/// - POST /v1/orders           → 注文一覧（年月指定）
/// - POST /v1/orders/detail    → 注文詳細
/// - POST /v1/invoices         → 請求書一覧（年月指定）
/// - GET  /home/orders/detail:{id}   → 注文書詳細ページ（PDFリンク抽出元）
/// - GET  /home/invoices/detail:{id} → 請求書詳細ページ（PDFリンク抽出元）
///
/// ## セッション切れの扱い
/// HTTP 401 は `EdiOasisError::Unauthorized` として区別される。呼び出し側
/// (mail_pipeline::phase2_fetch) はこれを見て1回だけ `login()` を呼び直してから
/// 同じ操作をリトライする（このファイル内では自動再ログインしない — 呼び出し元の
/// リトライ回数管理と統合するため）。
///
/// ## 参照元
/// EDI_MP: billing/services/edi_oasis_client.py
///   (fetch_orders/fetch_invoices/download_pdfs/download_order_pdf を移植)

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use tracing;

/// メール本文からEDI-OASIS注文書URLの detail_id を抽出する正規表現
static EDI_ORDER_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"https?://edi\.e-business\.co\.jp/home/orders/detail:(\d+)")
        .expect("EDI URL regex")
});

/// EDI-OASIS接続エラー
#[derive(Debug, thiserror::Error)]
pub enum EdiOasisError {
    #[error("EDI-OASIS認証情報が未設定です")]
    NotConfigured,

    #[error("EDI-OASISログイン失敗: {0}")]
    LoginFailed(String),

    #[error("EDI-OASIS API エラー: {0}")]
    ApiError(String),

    /// HTTP 401 — セッション切れ・トークン失効。
    /// 呼び出し側（Phase2）でこのバリアントを見て1回だけ再ログイン→リトライする。
    #[error("EDI-OASIS認証切れ: {0}")]
    Unauthorized(String),

    #[error("HTTP通信エラー: {0}")]
    HttpError(#[from] reqwest::Error),
}

/// HTTPステータスからエラーを分類する（401はUnauthorized、それ以外はApiError）
fn classify_api_error(status: reqwest::StatusCode, body: String, context: &str) -> EdiOasisError {
    let body_preview = if body.len() > 200 {
        let truncated = body.chars().take(200).collect::<String>();
        format!("{}…(truncated)", truncated)
    } else {
        body
    };
    if status == reqwest::StatusCode::UNAUTHORIZED {
        EdiOasisError::Unauthorized(format!("{context}: HTTP 401 — {body_preview}"))
    } else {
        EdiOasisError::ApiError(format!("{context}: HTTP {status} — {body_preview}"))
    }
}

/// EDI-OASIS REST API クライアント
pub struct EdiOasisClient {
    http: reqwest::Client,
    base_url: String,
    user: String,
    password: String,
    token: Option<String>,
}

/// ログインレスポンス
#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: Option<String>,
    message: Option<String>,
}

/// 注文一覧レスポンス
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OrdersResponse {
    orders_json: Option<Vec<serde_json::Value>>,
}

/// OASIS 注文サマリ（フロントエンドに返す用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderSummary {
    pub id: i64,
    pub order_no: String,
    pub is_received: bool,
    pub contract_type: String,
    pub year: String,
    pub month: String,
}

/// 請求書一覧レスポンス
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoicesResponse {
    invoices_json: Option<Vec<serde_json::Value>>,
}

/// OASIS 請求書サマリ（フロントエンドに返す用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceSummary {
    pub id: i64,
    pub invoice_no: String,
    pub is_confirmed: bool,
    pub year: String,
    pub month: String,
}

/// 請求書詳細（EDI-OASIS APIから取得）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceDetail {
    pub id: Option<i64>,
    #[serde(alias = "invoice_no", alias = "invoiceNo")]
    pub invoice_no: Option<String>,
    pub year: Option<i32>,
    pub month: Option<i32>,
    #[serde(alias = "work_start", alias = "workStart")]
    pub work_start: Option<String>,
    #[serde(alias = "work_end", alias = "workEnd")]
    pub work_end: Option<String>,
    #[serde(default)]
    pub items: Vec<InvoiceDetailItem>,
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// 請求書明細行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceDetailItem {
    /// 項目名（エンジニア名）
    #[serde(alias = "item_name", alias = "itemName", default)]
    pub item_name: String,
    /// 基本金額
    #[serde(alias = "item_basic_amount", alias = "itemBasicAmount", default)]
    pub item_basic_amount: f64,
    /// 合計金額
    #[serde(alias = "item_amount", alias = "itemAmount", default)]
    pub item_amount: f64,
    /// 税額
    #[serde(alias = "item_tax_amount", alias = "itemTaxAmount", default)]
    pub item_tax_amount: f64,
    /// 下限時間
    #[serde(alias = "item_min_hours", alias = "itemMinHours", default)]
    pub item_min_hours: f64,
    /// 上限時間
    #[serde(alias = "item_max_hours", alias = "itemMaxHours", default)]
    pub item_max_hours: f64,
    /// 控除単価（時間不足時）
    #[serde(alias = "item_minus_per_hour", alias = "itemMinusPerHour", default)]
    pub item_minus_per_hour: f64,
    /// 超過単価（時間超過時）
    #[serde(alias = "item_plus_per_hour", alias = "itemPlusPerHour", default)]
    pub item_plus_per_hour: f64,
    /// 単価
    #[serde(alias = "item_rate", alias = "itemRate", default)]
    pub item_rate: f64,
    /// 実稼働時間
    #[serde(alias = "item_total_hours", alias = "itemTotalHours", default)]
    pub item_total_hours: f64,
}

/// 注文詳細（EDI-OASIS APIが返すJSON — フィールド名はcamelCase）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetail {
    /// 注文ID（OASIS内部ID）
    pub id: Option<i64>,
    /// 注文番号
    #[serde(alias = "order_no", alias = "orderNo")]
    pub order_no: Option<String>,
    /// プロジェクト名
    #[serde(alias = "project_name", alias = "projectName")]
    pub project_name: Option<String>,
    /// 対象年
    pub year: Option<i32>,
    /// 対象月
    pub month: Option<i32>,
    /// 注文日
    #[serde(alias = "order_date", alias = "orderDate")]
    pub order_date: Option<String>,
    /// 作業開始日
    #[serde(alias = "work_start", alias = "workStart", alias = "start_date", alias = "startDate")]
    pub work_start: Option<String>,
    /// 作業終了日
    #[serde(alias = "work_end", alias = "workEnd", alias = "end_date", alias = "endDate")]
    pub work_end: Option<String>,
    /// 単価
    #[serde(alias = "unit_price", alias = "unitPrice")]
    pub unit_price: Option<i64>,
    /// 金額
    pub amount: Option<i64>,
    /// 税額
    #[serde(alias = "tax_amount", alias = "taxAmount")]
    pub tax_amount: Option<i64>,
    /// 担当者名
    #[serde(alias = "worker_name", alias = "workerName", alias = "engineer_name", alias = "engineerName")]
    pub worker_name: Option<String>,
    /// ステータス
    pub status: Option<String>,

    /// 生の JSON を保持（フィールドマッピング確認用）
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl EdiOasisClient {
    /// 環境変数から設定を読み込んでクライアントを作成
    pub fn from_env() -> Result<Self, EdiOasisError> {
        let base_url = std::env::var("EDI_OASIS_BASE_URL")
            .unwrap_or_else(|_| "https://edi.e-business.co.jp".to_string());
        let user = std::env::var("EDI_OASIS_USER").unwrap_or_default();
        let password = std::env::var("EDI_OASIS_PASSWORD").unwrap_or_default();

        if user.is_empty() || password.is_empty() {
            return Err(EdiOasisError::NotConfigured);
        }

        let http = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            http,
            base_url,
            user,
            password,
            token: None,
        })
    }

    /// JWT ログイン
    pub async fn login(&mut self) -> Result<(), EdiOasisError> {
        let url = format!("{}/v1/login", self.base_url);

        let resp = self.http
            .post(&url)
            .json(&serde_json::json!({
                "mail": self.user,
                "password": self.password,
            }))
            .send()
            .await?;

        if resp.status() != reqwest::StatusCode::OK {
            return Err(EdiOasisError::LoginFailed(
                format!("HTTP {}", resp.status())
            ));
        }

        let data: LoginResponse = resp.json().await?;

        match data.token {
            Some(token) if !token.is_empty() => {
                self.token = Some(token);
                tracing::info!("[EDI-OASIS] APIログイン成功");

                // ログイン通知（失敗しても無視）
                let _ = self.http
                    .post(&format!("{}/v1/login/inform", self.base_url))
                    .json(&serde_json::json!({"mail": self.user}))
                    .send()
                    .await;

                Ok(())
            }
            _ => {
                let msg = data.message.unwrap_or_else(|| "不明なエラー".to_string());
                Err(EdiOasisError::LoginFailed(msg))
            }
        }
    }

    /// 指定年月の注文一覧を取得（未受領フィルタ対応）
    pub async fn fetch_orders(&self, year: i32, month: i32) -> Result<Vec<OrderSummary>, EdiOasisError> {
        let token = self.token.as_ref()
            .ok_or_else(|| EdiOasisError::ApiError("未ログインです".to_string()))?;

        let resp = self.http
            .post(format!("{}/v1/orders", self.base_url))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "ctx_email": self.user,
                "year": year,
                "month": month,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "注文一覧取得失敗"));
        }

        let data: OrdersResponse = resp.json().await
            .map_err(|e| EdiOasisError::ApiError(format!("JSON パースエラー: {e}")))?;

        let raw_list = data.orders_json.unwrap_or_default();
        let mut orders = Vec::new();

        for val in raw_list {
            // ordersJson は文字列の場合もある
            let obj = if val.is_string() {
                serde_json::from_str::<serde_json::Value>(val.as_str().unwrap_or("{}"))
                    .unwrap_or(val)
            } else {
                val
            };

            let summary = OrderSummary {
                id: obj.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                order_no: obj.get("order_no").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                is_received: obj.get("is_received").and_then(|v| v.as_bool()).unwrap_or(false),
                contract_type: obj.get("contract_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                year: obj.get("year").and_then(|v| v.as_str().or_else(|| v.as_i64().map(|_| ""))).unwrap_or("").to_string(),
                month: obj.get("month").and_then(|v| v.as_str().or_else(|| v.as_i64().map(|_| ""))).unwrap_or("").to_string(),
            };
            orders.push(summary);
        }

        tracing::info!("[EDI-OASIS] 注文一覧取得: {}件", orders.len());
        Ok(orders)
    }

    /// 注文詳細を JSON で取得
    pub async fn fetch_order_detail(&self, order_id: i64) -> Result<OrderDetail, EdiOasisError> {
        let token = self.token.as_ref()
            .ok_or_else(|| EdiOasisError::ApiError("未ログインです".to_string()))?;

        let url = format!("{}/v1/orders/detail", self.base_url);

        let resp = self.http
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "id": order_id,
                "ctx_email": self.user,
            }))
            .send()
            .await?;

        if resp.status() != reqwest::StatusCode::OK {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "注文詳細取得失敗"));
        }

        let body_text = resp.text().await
            .map_err(|e| EdiOasisError::ApiError(format!("レスポンス読取エラー: {e}")))?;

        // 生レスポンスをログ出力（フィールドマッピングのデバッグ用、先頭約2000バイト）
        let preview = if body_text.len() > 2000 {
            // UTF-8文字境界を考慮した安全な切断
            let mut end = 2000;
            while !body_text.is_char_boundary(end) { end -= 1; }
            &body_text[..end]
        } else {
            &body_text
        };
        tracing::info!("[EDI-OASIS] 注文詳細 生レスポンス (id={}): {}", order_id, preview);

        // orderJson フィールドがある場合、Go map[] 混在JSONのため手動抽出
        let detail: OrderDetail = if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(&body_text) {
            if let Some(order_json_str) = wrapper.get("orderJson").and_then(|v| v.as_str()) {
                tracing::info!("[EDI-OASIS] orderJson フィールド発見、手動抽出");
                // orderJson は Go fmt.Sprint の map[] を含むため標準JSONパース不可
                // header ブロック（正規JSON）を抽出してパースする
                Self::parse_oasis_order_json(order_json_str, order_id)
            } else if let Some(order_json_obj) = wrapper.get("orderJson") {
                serde_json::from_value(order_json_obj.clone())
                    .map_err(|e| EdiOasisError::ApiError(format!("orderJson オブジェクトパースエラー: {e}")))?
            } else {
                serde_json::from_str(&body_text)
                    .map_err(|e| EdiOasisError::ApiError(format!("JSON パースエラー: {e}")))?
            }
        } else {
            serde_json::from_str(&body_text)
                .map_err(|e| EdiOasisError::ApiError(format!("JSON パースエラー: {e}")))?
        };

        tracing::info!(
            "[EDI-OASIS] 注文詳細取得成功: id={}, order_no={:?}, project={:?}, worker={:?}, year={:?}, month={:?}",
            order_id,
            detail.order_no,
            detail.project_name,
            detail.worker_name,
            detail.year,
            detail.month,
        );

        Ok(detail)
    }

    /// OASIS orderJson（Go map[] 混在）からOrderDetailを手動抽出
    ///
    /// orderJson は以下のような構造:
    /// - トップレベル: contract_type, end_year, header, details, ...
    /// - header: Go の <nil> 混在JSON（正規JSONパース不可）
    /// - details: Go の map[key:value ...] 形式（item_name, item_basic_amount 等）
    fn parse_oasis_order_json(raw: &str, order_id: i64) -> OrderDetail {
        use regex::Regex;

        // 全フィールドを正規表現で抽出（headerにも<nil>が含まれるためJSONパース不可）
        let extract_str_field = |key: &str| -> Option<String> {
            let re = Regex::new(&format!(r#""{}"[:\s]*"([^"]+)""#, regex::escape(key))).ok()?;
            re.captures(raw)?.get(1).map(|m| m.as_str().to_string())
        };

        let order_no = extract_str_field("order_no");
        let work_start = extract_str_field("work_start_date");
        let work_end = extract_str_field("work_end_date");
        let contract_name = extract_str_field("contract_name");
        let order_date = extract_str_field("order_date");
        let end_year_str = extract_str_field("end_year");
        let end_month_str = extract_str_field("end_month");

        // year/month を推定
        let year = end_year_str.and_then(|y| y.parse::<i32>().ok())
            .or_else(|| work_start.as_ref().and_then(|ws| ws.get(..4)?.parse().ok()));
        let month = end_month_str.and_then(|m| m.parse::<i32>().ok())
            .or_else(|| work_start.as_ref().and_then(|ws| ws.get(5..7)?.parse().ok()));

        // details 内の item_name を抽出（Go map[] 形式: item_name:前野 謙）
        let worker_name = {
            let re = Regex::new(r"item_name:([^\s\]]+(?:\s[^\s\]]+)?)").ok();
            re.and_then(|r| r.captures(raw))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
        };

        // details 内の item_basic_amount を抽出（基本単価）
        let unit_price = {
            let re = Regex::new(r"item_basic_amount:(\d+)").ok();
            re.and_then(|r| r.captures(raw))
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<i64>().ok())
        };

        // details 内の item_amount を抽出（合計金額。invoiceJsonのdetailsと同じキー体系）。
        // 前方に空白/先頭を要求し、item_basic_amount等の部分文字列に誤マッチしないようにする
        let amount = {
            let re = Regex::new(r"(?:^|\s)item_amount:(\d+)").ok();
            re.and_then(|r| r.captures(raw))
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<i64>().ok())
        };

        tracing::info!(
            "[EDI-OASIS] orderJson手動抽出: order_no={:?}, contract={:?}, worker={:?}, year={:?}, month={:?}, work_start={:?}, unit_price={:?}, amount={:?}",
            order_no, contract_name, worker_name, year, month, work_start, unit_price, amount,
        );

        OrderDetail {
            id: Some(order_id),
            order_no,
            project_name: contract_name,
            year,
            month,
            order_date,
            work_start,
            work_end,
            unit_price,
            amount,
            tax_amount: None,
            worker_name,
            status: extract_str_field("status"),
            extra: std::collections::HashMap::new(),
        }
    }

    /// OASIS 上で注文書を「受領」する
    ///
    /// DevTools で確認した API 仕様:
    /// POST /v1/orders/approve
    /// {
    ///   "id": order_id,
    ///   "ctx_email": "y.yoshikawa@example.com",
    ///   "appr_email": "y.yoshikawa@example.com",
    ///   "ctx_mail": "y.yoshikawa@example.com",
    ///   "fullName": "有限会社マックプランニング",
    ///   "full_name": "有限会社マックプランニング"
    /// }
    pub async fn approve_order(&self, order_id: i64) -> Result<(), EdiOasisError> {
        let token = self.token.as_deref()
            .ok_or_else(|| EdiOasisError::LoginFailed("未ログイン".into()))?;

        // 会社名を環境変数から取得（デフォルト: 有限会社マックプランニング）
        let company_name = std::env::var("EDI_OASIS_COMPANY_NAME")
            .unwrap_or_else(|_| "有限会社マックプランニング".to_string());

        let payload = serde_json::json!({
            "id": order_id,
            "ctx_email": self.user,
            "appr_email": self.user,
            "ctx_mail": self.user,
            "fullName": company_name,
            "full_name": company_name,
        });

        tracing::info!("[EDI-OASIS] 注文書受領: id={}", order_id);

        let resp = self.http.post(format!("{}/v1/orders/approve", self.base_url))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "注文書受領失敗"));
        }

        tracing::info!("[EDI-OASIS] 注文書受領完了: id={}", order_id);
        Ok(())
    }

    /// 指定年月の請求書一覧を取得
    pub async fn fetch_invoices(&self, year: i32, month: i32) -> Result<Vec<InvoiceSummary>, EdiOasisError> {
        let token = self.token.as_ref()
            .ok_or_else(|| EdiOasisError::ApiError("未ログインです".to_string()))?;

        let resp = self.http
            .post(format!("{}/v1/invoices", self.base_url))
            .bearer_auth(token)
            .json(&serde_json::json!({
                "mail": self.user,
                "year": year,
                "month": month,
            }))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "請求書一覧取得失敗"));
        }

        let data: InvoicesResponse = resp.json().await
            .map_err(|e| EdiOasisError::ApiError(format!("JSON パースエラー: {e}")))?;

        let raw_list = data.invoices_json.unwrap_or_default();
        let mut invoices = Vec::new();

        for val in raw_list {
            let obj = if val.is_string() {
                serde_json::from_str::<serde_json::Value>(val.as_str().unwrap_or("{}"))
                    .unwrap_or(val)
            } else {
                val
            };

            let summary = InvoiceSummary {
                id: obj.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                invoice_no: obj.get("invoice_no").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                is_confirmed: obj.get("is_confirmed").and_then(|v| v.as_bool()).unwrap_or(false),
                year: obj.get("year").and_then(|v| v.as_str().or_else(|| v.as_i64().map(|_| ""))).unwrap_or("").to_string(),
                month: obj.get("month").and_then(|v| v.as_str().or_else(|| v.as_i64().map(|_| ""))).unwrap_or("").to_string(),
            };
            invoices.push(summary);
        }

        tracing::info!("[EDI-OASIS] 請求書一覧取得: {}件", invoices.len());
        Ok(invoices)
    }

    /// OASIS 上で請求書を「承諾」する（確定）
    ///
    /// Django版EDI_MPの approve() を移植。
    /// POST /v1/invoices/approve
    pub async fn approve_invoice(&self, invoice_id: i64) -> Result<(), EdiOasisError> {
        let token = self.token.as_deref()
            .ok_or_else(|| EdiOasisError::LoginFailed("未ログイン".into()))?;

        let company_name = std::env::var("EDI_OASIS_COMPANY_NAME")
            .unwrap_or_else(|_| "有限会社マックプランニング".to_string());

        let payload = serde_json::json!({
            "id": invoice_id,
            "ctx_email": self.user,
            "appr_email": self.user,
            "ctx_mail": self.user,
            "fullName": company_name,
            "full_name": company_name,
        });

        tracing::info!("[EDI-OASIS] 請求書承諾: id={}", invoice_id);

        let resp = self.http.post(format!("{}/v1/invoices/approve", self.base_url))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "請求書承諾失敗"));
        }

        tracing::info!("[EDI-OASIS] 請求書承諾完了: id={}", invoice_id);
        Ok(())
    }

    /// 注文書の印刷用HTMLをダウンロードする
    ///
    /// OASIS の `/home/orders/print:{id}` は HTML を返す（PDF直接ではない）
    pub async fn download_order_html(&self, detail_id: i64) -> Result<Vec<u8>, EdiOasisError> {
        let token = self.token.as_deref()
            .ok_or_else(|| EdiOasisError::LoginFailed("未ログイン".into()))?;

        let url = format!("{}/home/orders/print:{}", self.base_url, detail_id);
        tracing::info!("[EDI-OASIS] 注文書HTML取得: {}", url);

        let resp = self.http.get(&url)
            .bearer_auth(token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(classify_api_error(status, String::new(), "注文書HTML取得失敗"));
        }

        let bytes = resp.bytes().await
            .map_err(|e| EdiOasisError::ApiError(format!("HTML読込エラー: {e}")))?;

        tracing::info!("[EDI-OASIS] 注文書HTML取得完了: {} bytes", bytes.len());
        Ok(bytes.to_vec())
    }

    /// 請求書の印刷用HTMLをダウンロードする
    pub async fn download_invoice_html(&self, invoice_id: i64) -> Result<Vec<u8>, EdiOasisError> {
        let token = self.token.as_deref()
            .ok_or_else(|| EdiOasisError::LoginFailed("未ログイン".into()))?;

        let url = format!("{}/home/invoices/print:{}", self.base_url, invoice_id);
        tracing::info!("[EDI-OASIS] 請求書HTML取得: {}", url);

        let resp = self.http.get(&url)
            .bearer_auth(token)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            return Err(classify_api_error(status, String::new(), "請求書HTML取得失敗"));
        }

        let bytes = resp.bytes().await
            .map_err(|e| EdiOasisError::ApiError(format!("HTML読込エラー: {e}")))?;

        tracing::info!("[EDI-OASIS] 請求書HTML取得完了: {} bytes", bytes.len());
        Ok(bytes.to_vec())
    }

    /// メール本文から EDI-OASIS 注文書の detail_id を抽出する
    pub fn extract_order_detail_id(body: &str) -> Option<i64> {
        EDI_ORDER_URL_RE.captures(body)
            .and_then(|cap| cap.get(1))
            .and_then(|m| m.as_str().parse::<i64>().ok())
    }

    /// メール本文がEDI注文書通知かどうかを判定する
    pub fn is_edi_order_email(body: &str) -> bool {
        body.contains("edi.e-business.co.jp")
            && (body.contains("注文書") || body.contains("注文") || body.contains("order"))
    }

    /// 請求書の詳細JSONを取得してパース
    ///
    /// EDI_MP_1（旧Python版）の `billing/services/edi_oasis_client.py::fetch_invoice_detail` を移植。
    /// `POST /v1/invoices/detail` に `{id, ctx_email}` を送る（`/v1/orders/detail` と対になるエンドポイント）。
    /// 旧実装は誤って `GET /api/invoices/{id}`（存在しないパス）を叩いており、200+HTMLのフロントエンド
    /// フォールバックページが返ってきてJSONパースに失敗し続けていた。
    pub async fn fetch_invoice_detail(&mut self, invoice_id: i64) -> Result<InvoiceDetail, EdiOasisError> {
        let token = self.token.as_ref()
            .ok_or_else(|| EdiOasisError::ApiError("未ログインです".to_string()))?;

        let url = format!("{}/v1/invoices/detail", self.base_url);

        let res = self.http
            .post(&url)
            .bearer_auth(token)
            .json(&serde_json::json!({
                "id": invoice_id,
                "ctx_email": self.user,
            }))
            .send()
            .await
            .map_err(|e| EdiOasisError::ApiError(format!("HTTP error: {}", e)))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "請求書詳細取得失敗"));
        }

        let body = res.text().await
            .map_err(|e| EdiOasisError::ApiError(format!("Response read error: {}", e)))?;

        // OASIS側でセッションが切れると、401ではなく200+ログインページのHTMLを返すことがある。
        // JSONパースエラーとして握りつぶさず、再ログインでリトライ可能なUnauthorizedとして扱う。
        if body.trim_start().starts_with("<!DOCTYPE") || body.trim_start().starts_with("<html") {
            return Err(EdiOasisError::Unauthorized(format!(
                "請求書詳細取得: HTMLが返却されました。セッション切れの可能性があります (body={})",
                &body[..body.len().min(200)]
            )));
        }

        // 生レスポンスをログ出力（フィールドマッピングのデバッグ用、先頭約2000バイト。fetch_order_detailと同じ方針）
        let preview = if body.len() > 2000 {
            let mut end = 2000;
            while !body.is_char_boundary(end) { end -= 1; }
            &body[..end]
        } else {
            &body
        };
        tracing::info!("[EDI-OASIS] 請求書詳細 生レスポンス (id={}): {}", invoice_id, preview);

        // invoiceJson フィールドがある場合、Go map[] 混在JSONのため手動抽出（orderJsonと同じ形式）
        let detail: InvoiceDetail = if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(invoice_json_str) = wrapper.get("invoiceJson").and_then(|v| v.as_str()) {
                tracing::info!("[EDI-OASIS] invoiceJson フィールド発見（文字列）、手動抽出");
                Self::parse_oasis_invoice_json(invoice_json_str, invoice_id)
            } else if let Some(invoice_json_obj) = wrapper.get("invoiceJson").filter(|v| v.is_object()) {
                serde_json::from_value(invoice_json_obj.clone())
                    .map_err(|e| EdiOasisError::ApiError(format!("invoiceJson オブジェクトパースエラー: {e} (body={})", &body[..body.len().min(200)])))?
            } else {
                serde_json::from_str(&body)
                    .map_err(|e| EdiOasisError::ApiError(format!("JSON parse error: {} (body={})", e, &body[..body.len().min(200)])))?
            }
        } else {
            serde_json::from_str(&body)
                .map_err(|e| EdiOasisError::ApiError(format!("JSON parse error: {} (body={})", e, &body[..body.len().min(200)])))?
        };

        tracing::info!(
            "[EDI-OASIS] 請求書詳細取得成功: id={}, year={:?}, month={:?}, items={}",
            invoice_id, detail.year, detail.month, detail.items.len(),
        );

        Ok(detail)
    }

    /// OASIS invoiceJson（Go map[] 混在）からInvoiceDetailを手動抽出
    ///
    /// invoiceJson は以下のような構造の文字列（標準JSONパース不可）:
    /// - トップレベル: id, year, month, tax_rate, details, ... （ここはJSON）
    /// - details: Go の fmt.Sprint によるスライス `[map[key:val ...] map[key:val ...] ...]`
    ///   （エンジニアごとの明細1件が1つの `map[...]` ブロックに対応する）
    fn parse_oasis_invoice_json(raw: &str, invoice_id: i64) -> InvoiceDetail {
        use regex::Regex;

        let extract_str_field = |key: &str| -> Option<String> {
            let re = Regex::new(&format!(r#""{}"[:\s]*"([^"]+)""#, regex::escape(key))).ok()?;
            re.captures(raw)?.get(1).map(|m| m.as_str().to_string())
        };

        let year = extract_str_field("year").and_then(|y| y.parse::<i32>().ok());
        let month = extract_str_field("month").and_then(|m| m.parse::<i32>().ok());

        // details配列内の各 map[...] ブロックを角括弧の深さを追いながら分割する
        // （project_members:[10651] のようなネストした[]を含むため、非貪欲正規表現では分割できない）
        let mut items = Vec::new();
        let mut search_from = 0usize;
        while let Some(rel_pos) = raw[search_from..].find("map[") {
            let start = search_from + rel_pos + "map[".len();
            let mut depth = 1i32;
            let mut end = None;
            for (offset, ch) in raw[start..].char_indices() {
                match ch {
                    '[' => depth += 1,
                    ']' => {
                        depth -= 1;
                        if depth == 0 {
                            end = Some(start + offset);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(end) = end else { break };
            items.push(Self::parse_oasis_invoice_item_block(&raw[start..end]));
            search_from = end;
        }

        tracing::info!(
            "[EDI-OASIS] invoiceJson手動抽出: invoice_id={}, year={:?}, month={:?}, items={}件",
            invoice_id, year, month, items.len(),
        );

        InvoiceDetail {
            id: Some(invoice_id),
            invoice_no: None,
            year,
            month,
            work_start: None,
            work_end: None,
            items,
            extra: std::collections::HashMap::new(),
        }
    }

    /// invoiceJsonのdetails内、1エンジニア分の `map[key:val ...]` ブロックからInvoiceDetailItemを抽出
    fn parse_oasis_invoice_item_block(block: &str) -> InvoiceDetailItem {
        use regex::Regex;

        // 値はスペース区切り。氏名（例: "前野 謙"）のようにスペースを含む値にも対応するため、
        // 直後のトークンまでを1回だけ許容で取り込む（parse_oasis_order_jsonのworker_name抽出と同じ方針）
        let extract_str = |key: &str| -> Option<String> {
            let re = Regex::new(&format!(r"(?:^|\s){}:([^\s\]]+(?:\s[^\s\]]+)?)", regex::escape(key))).ok()?;
            re.captures(block)?.get(1).map(|m| m.as_str().trim().to_string())
        };
        // 前方に空白/先頭を要求することで、item_amount が item_basic_amount 等の部分文字列に
        // 誤マッチしないようにする
        let extract_f64 = |key: &str| -> f64 {
            let re = Regex::new(&format!(r"(?:^|\s){}:(-?\d+(?:\.\d+)?)", regex::escape(key))).ok();
            re.and_then(|r| r.captures(block))
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<f64>().ok())
                .unwrap_or(0.0)
        };

        InvoiceDetailItem {
            item_name: extract_str("item_name").unwrap_or_default(),
            item_basic_amount: extract_f64("item_basic_amount"),
            item_amount: extract_f64("item_amount"),
            item_tax_amount: extract_f64("item_tax_amount"),
            item_min_hours: extract_f64("item_min_hours"),
            item_max_hours: extract_f64("item_max_hours"),
            item_minus_per_hour: extract_f64("item_minus_per_hour"),
            item_plus_per_hour: extract_f64("item_plus_per_hour"),
            item_rate: extract_f64("item_rate"),
            item_total_hours: extract_f64("item_total_hours"),
        }
    }

    // ──────────────────────────────────────────
    // PDFダウンロード（EDI_MP billing/services/edi_oasis_client.py の
    // download_pdfs / download_order_pdf を移植）
    // ──────────────────────────────────────────

    /// 注文書詳細ページから注文書PDFをダウンロードする
    ///
    /// OASISの詳細ページHTMLをGETし、PDFダウンロードリンクを正規表現で抽出してから
    /// 実際のPDFバイナリを取得する。リンクが見つからない場合は推測URLにフォールバックする。
    pub async fn download_order_pdf(&self, detail_id: i64) -> Result<Option<Vec<u8>>, EdiOasisError> {
        let token = self.token.as_deref()
            .ok_or_else(|| EdiOasisError::LoginFailed("未ログイン".into()))?;

        let detail_url = format!("{}/home/orders/detail:{}", self.base_url, detail_id);
        let resp = self.http.get(&detail_url).bearer_auth(token).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "注文書詳細ページアクセス失敗"));
        }
        let html = resp.text().await
            .map_err(|e| EdiOasisError::ApiError(format!("HTML読込エラー: {e}")))?;

        let pdf_url = Self::extract_pdf_link(
            &html,
            detail_id,
            &[
                "print", "order.*pdf", "order.*print", "注文書", "発注書",
            ],
        ).unwrap_or_else(|| format!("{}/home/orders/print:{}", self.base_url, detail_id));

        self.download_pdf_bytes(&pdf_url, token).await
    }

    /// 請求書詳細ページから請求書PDF・支払通知書PDFをダウンロードする
    ///
    /// Returns: (invoice_pdf, payment_notice_pdf)
    pub async fn download_invoice_pdfs(
        &self,
        detail_id: i64,
    ) -> Result<(Option<Vec<u8>>, Option<Vec<u8>>), EdiOasisError> {
        let token = self.token.as_deref()
            .ok_or_else(|| EdiOasisError::LoginFailed("未ログイン".into()))?;

        let detail_url = format!("{}/home/invoices/detail:{}", self.base_url, detail_id);
        let resp = self.http.get(&detail_url).bearer_auth(token).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "請求書詳細ページアクセス失敗"));
        }
        let html = resp.text().await
            .map_err(|e| EdiOasisError::ApiError(format!("HTML読込エラー: {e}")))?;

        let invoice_url = Self::extract_pdf_link(
            &html,
            detail_id,
            &["print", "invoice.*pdf", "invoice.*print", "請求書"],
        ).unwrap_or_else(|| format!("{}/home/invoices/print:{}", self.base_url, detail_id));
        let payment_url = Self::extract_pdf_link(
            &html,
            detail_id,
            &["payment.*pdf", "payment.*print", "支払通知"],
        );

        let invoice_pdf = self.download_pdf_bytes(&invoice_url, token).await.unwrap_or(None);
        let payment_notice_pdf = match payment_url {
            Some(url) => self.download_pdf_bytes(&url, token).await.unwrap_or(None),
            None => None,
        };

        Ok((invoice_pdf, payment_notice_pdf))
    }

    /// 詳細ページHTMLからPDFダウンロードリンクを抽出する（複数パターンをフォールバック順に試す）
    fn extract_pdf_link(html: &str, detail_id: i64, patterns: &[&str]) -> Option<String> {
        for pat in patterns {
            let re = Regex::new(&format!(
                r#"href=["']([^"']*{}[^"']*)["']"#,
                pat.replace(' ', "")
            )).ok()?;
            if let Some(cap) = re.captures(html) {
                let url = cap.get(1)?.as_str();
                let _ = detail_id; // detail_idはログ用途のみ、URLには含めない場合もある
                return Some(if url.starts_with("http") {
                    url.to_string()
                } else {
                    format!("https://edi.e-business.co.jp{url}")
                });
            }
        }
        None
    }

    /// PDF URLからバイナリを取得する（Content-TypeがPDFでない場合はNoneを返す）
    async fn download_pdf_bytes(&self, url: &str, token: &str) -> Result<Option<Vec<u8>>, EdiOasisError> {
        let resp = self.http.get(url).bearer_auth(token).send().await?;
        if !resp.status().is_success() {
            tracing::warn!("[EDI-OASIS] PDFダウンロード失敗: {} (HTTP {})", url, resp.status());
            return Ok(None);
        }

        let is_pdf = resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.starts_with("application/pdf"))
            .unwrap_or(false);

        let bytes = resp.bytes().await
            .map_err(|e| EdiOasisError::ApiError(format!("PDF読込エラー: {e}")))?;

        if !is_pdf {
            tracing::warn!("[EDI-OASIS] PDF応答がPDFでない: {} ({} bytes)", url, bytes.len());
            return Ok(None);
        }

        tracing::info!("[EDI-OASIS] PDFダウンロード成功: {} ({} bytes)", url, bytes.len());
        Ok(Some(bytes.to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_order_detail_id() {
        let body = "注文書を登録しました。\nhttps://edi.e-business.co.jp/home/orders/detail:12345\n確認してください。";
        assert_eq!(EdiOasisClient::extract_order_detail_id(body), Some(12345));
    }

    #[test]
    fn test_extract_order_detail_id_none() {
        let body = "普通のメール本文です。URLはありません。";
        assert_eq!(EdiOasisClient::extract_order_detail_id(body), None);
    }

    #[test]
    fn test_is_edi_order_email() {
        assert!(EdiOasisClient::is_edi_order_email(
            "edi.e-business.co.jp の注文書を登録しました"
        ));
        assert!(!EdiOasisClient::is_edi_order_email(
            "稼働報告書をお送りします"
        ));
    }

    #[test]
    fn classify_api_error_401_is_unauthorized() {
        let err = classify_api_error(reqwest::StatusCode::UNAUTHORIZED, "expired".into(), "取得失敗");
        assert!(matches!(err, EdiOasisError::Unauthorized(_)));
    }

    #[test]
    fn classify_api_error_other_is_api_error() {
        let err = classify_api_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "boom".into(), "取得失敗");
        assert!(matches!(err, EdiOasisError::ApiError(_)));
    }

    /// ステージングでOASIS APIから実際に取得したinvoiceJson(id=13383)の生データ（2件目までの完全な部分）を使用
    #[test]
    fn parse_oasis_invoice_json_extracts_engineer_items() {
        let raw = r#"{"year":"2026","tax_rate":"0.10","turnover_amount":2460000,"id":13383,"month":"06","details":[map[id:29440 is_blanket_contract:false is_hourly_pay:false item_amount:880000 item_basic_amount:800000 item_comment:<nil> item_expense_amount:0 item_extra_hours:0.00 item_max_hours:200.00 item_min_hours:140.00 item_min_max:140.00/200.00 item_minus_per_hour:5710 item_minus_per_hour_memo:不足単価：￥5,710/h item_name:前野 謙 item_no:1 item_other_amount:0 item_overtime_amount:0 item_plus_per_hour:4000 item_plus_per_hour_memo:超過単価：￥4,000/h item_rate:1.0 item_tax_amount:80000 item_total_hours:160.00 item_turnover_amount:800000 member_content_type:49 member_object_id:30858 member_type:10 month:06 monthly_request:563183 order:25384 project:2439 project_members:[10651] request:13383 request_no:2606123 simple_member_type:02 year:2026] map[id:29441 is_blanket_contract:false is_hourly_pay:false item_amount:924000 item_basic_amount:840000 item_comment:<nil> item_expense_amount:0 item_extra_hours:0.00 item_max_hours:190.00 item_min_hours:140.00 item_min_max:140.00/190.00 item_minus_per_hour:6000 item_minus_per_hour_memo:不足単価：￥6,000/h item_name:吉川 裕 item_no:2 item_other_amount:0 item_overtime_amount:0 item_plus_per_hour:4420 item_plus_per_hour_memo:超過単価：￥4,420/h item_rate:1.0 item_tax_amount:84000 item_total_hours:160.00 item_turnover_amount:840000 member_content_type:49 member_object_id:31005 member_type:10 month:06 monthly_request:534994 order:25383 project:2439 project_members:[11243] request:13383 request_no:2606123 simple_member_type:02 year:2026]]}"#;

        let detail = EdiOasisClient::parse_oasis_invoice_json(raw, 13383);

        assert_eq!(detail.year, Some(2026));
        assert_eq!(detail.month, Some(6));
        assert_eq!(detail.items.len(), 2);

        let item0 = &detail.items[0];
        assert_eq!(item0.item_name, "前野 謙");
        assert_eq!(item0.item_amount, 880000.0);
        assert_eq!(item0.item_basic_amount, 800000.0);
        assert_eq!(item0.item_tax_amount, 80000.0);
        assert_eq!(item0.item_min_hours, 140.0);
        assert_eq!(item0.item_max_hours, 200.0);
        assert_eq!(item0.item_minus_per_hour, 5710.0);
        assert_eq!(item0.item_plus_per_hour, 4000.0);
        assert_eq!(item0.item_rate, 1.0);
        assert_eq!(item0.item_total_hours, 160.0);

        let item1 = &detail.items[1];
        assert_eq!(item1.item_name, "吉川 裕");
        assert_eq!(item1.item_amount, 924000.0);
        assert_eq!(item1.item_basic_amount, 840000.0);
    }

    /// orderJson（Go map[] 混在形式）からitem_amountが正しく抽出できることを確認する回帰テスト。
    /// parse_oasis_invoice_json_extracts_engineer_items で使った実データ(id=29440,前野謙,
    /// item_basic_amount:800000/item_amount:880000)と同じ値の系列を使い、注文側の
    /// parse_oasis_order_json でも同様に抽出できることを検証する（HANDOVER記載の
    /// 「item_amount抽出の新規取込での動作は未検証」の解消。実際のOASIS注文JSONは
    /// 2026年7月時点でまだ新規取込が発生しておらず実データでの確認はできていないため、
    /// 既知の実測値をベースにしたコードレベルの検証に留める）。
    #[test]
    fn parse_oasis_order_json_extracts_item_amount() {
        let raw = r#"{"contract_type":"1","end_year":"2026","end_month":"6","order_no":"SP20260601000001","order_date":"2026-05-15","work_start_date":"2026-06-01","work_end_date":"2026-06-30","contract_name":"横須賀市上下水道局の給排水設備工事等電子申請システム改修","header":<nil>,"status":"1","details":[map[id:29440 is_blanket_contract:false is_hourly_pay:false item_amount:880000 item_basic_amount:800000 item_comment:<nil> item_expense_amount:0 item_extra_hours:0.00 item_max_hours:200.00 item_min_hours:140.00 item_min_max:140.00/200.00 item_minus_per_hour:5710 item_minus_per_hour_memo:不足単価：￥5,710/h item_name:前野 謙 item_no:1 item_other_amount:0 item_overtime_amount:0 item_plus_per_hour:4000 item_plus_per_hour_memo:超過単価：￥4,000/h item_rate:1.0 item_tax_amount:80000 item_total_hours:160.00 item_turnover_amount:800000 member_content_type:49 member_object_id:30858 member_type:10 month:06 order:25384 project:2439 project_members:[10651] simple_member_type:02 year:2026]]}"#;

        let detail = EdiOasisClient::parse_oasis_order_json(raw, 25384);

        assert_eq!(detail.year, Some(2026));
        assert_eq!(detail.month, Some(6));
        assert_eq!(detail.order_no.as_deref(), Some("SP20260601000001"));
        assert_eq!(detail.worker_name.as_deref(), Some("前野 謙"));
        assert_eq!(detail.unit_price, Some(800000));
        assert_eq!(detail.amount, Some(880000));
    }

    #[test]
    fn extract_pdf_link_finds_matching_href() {
        let html = r#"<a href="/home/orders/print:99">注文書印刷</a>"#;
        let url = EdiOasisClient::extract_pdf_link(html, 99, &["print"]);
        assert_eq!(url, Some("https://edi.e-business.co.jp/home/orders/print:99".to_string()));
    }

    #[test]
    fn extract_pdf_link_returns_none_when_no_match() {
        let html = r#"<a href="/home/other">その他</a>"#;
        let url = EdiOasisClient::extract_pdf_link(html, 99, &["print", "pdf"]);
        assert_eq!(url, None);
    }
}
