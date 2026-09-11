/// infrastructure/peppol_client.rs — Peppol認定プロバイダー REST APIクライアント
///
/// 外部Peppolプロバイダーとの通信を担う。プロバイダー未確定のため、
/// 送信ペイロードは domain::services::jp_pint_mapper が生成する中間JSON構造体
/// （JpPintInvoice / JpPintSelfBillingInvoice）をそのままPOSTする実装とし、
/// 実プロバイダーのAPI形式が判明した時点でリクエスト整形部分のみ調整する。
///
/// エラー分類・from_env()での設定読込は edi_oasis_client.rs と同じパターン。

use serde::{Deserialize, Serialize};

use crate::domain::services::jp_pint_mapper::{JpPintInvoice, JpPintSelfBillingInvoice};

#[derive(Debug, thiserror::Error)]
pub enum PeppolError {
    #[error("Peppol API未設定です（PEPPOL_API_BASE_URL / PEPPOL_API_KEYを確認してください）")]
    NotConfigured,

    #[error("Peppolバリデーションエラー: {0}")]
    ValidationError(String),

    #[error("Peppol APIエラー: {0}")]
    ApiError(String),

    #[error("Peppol認証エラー: {0}")]
    Unauthorized(String),

    #[error("HTTP通信エラー: {0}")]
    HttpError(#[from] reqwest::Error),
}

/// HTTPステータスからエラーを分類する（401/403はUnauthorized、422はValidationError、それ以外はApiError）
fn classify_api_error(status: reqwest::StatusCode, body: String, context: &str) -> PeppolError {
    match status {
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => {
            PeppolError::Unauthorized(format!("{context}: HTTP {status} — {body}"))
        }
        reqwest::StatusCode::UNPROCESSABLE_ENTITY | reqwest::StatusCode::BAD_REQUEST => {
            PeppolError::ValidationError(format!("{context}: HTTP {status} — {body}"))
        }
        _ => PeppolError::ApiError(format!("{context}: HTTP {status} — {body}")),
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PeppolSendResponse {
    pub message_id: String,
    pub status: String,
}

pub struct PeppolClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
}

impl PeppolClient {
    /// 環境変数から設定を読み込んでクライアントを作成
    /// PEPPOL_API_BASE_URL / PEPPOL_API_KEY / PEPPOL_OWN_PARTICIPANT_ID
    pub fn from_env() -> Result<Self, PeppolError> {
        let base_url = std::env::var("PEPPOL_API_BASE_URL").unwrap_or_default();
        let api_key = std::env::var("PEPPOL_API_KEY").unwrap_or_default();

        if base_url.is_empty() || api_key.is_empty() {
            return Err(PeppolError::NotConfigured);
        }

        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self { http, base_url, api_key })
    }

    /// 自社のPeppol参加者ID（送信元）
    pub fn own_participant_id() -> String {
        std::env::var("PEPPOL_OWN_PARTICIPANT_ID").unwrap_or_default()
    }

    /// 売上請求書を送信する
    pub async fn send_invoice(&self, payload: &JpPintInvoice) -> Result<PeppolSendResponse, PeppolError> {
        self.post_document("/v1/invoices", payload).await
    }

    /// 仕入明細書（セルフビリング）を送信する
    pub async fn send_self_billing(&self, payload: &JpPintSelfBillingInvoice) -> Result<PeppolSendResponse, PeppolError> {
        self.post_document("/v1/self-billing-invoices", payload).await
    }

    async fn post_document<T: serde::Serialize>(&self, path: &str, payload: &T) -> Result<PeppolSendResponse, PeppolError> {
        let resp = self.http
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.api_key)
            .json(payload)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(classify_api_error(status, body, "Peppol文書送信失敗"));
        }

        resp.json::<PeppolSendResponse>().await
            .map_err(|e| PeppolError::ApiError(format!("JSONパースエラー: {e}")))
    }
}
