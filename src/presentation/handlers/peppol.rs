/// presentation/handlers/peppol.rs — Peppol送信・送受信ログ（Admin限定）
///
/// ## エンドポイント
/// - POST /api/v1/invoices/{id}/peppol/send — 売上請求書をPeppol経由で送信
/// - POST /api/v1/notices/{id}/peppol/send  — 支払通知書（セルフビリング）をPeppol経由で送信
/// - GET  /api/v1/peppol/transmissions      — 送受信ログ一覧（既定は未処理のみ）

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use crate::infrastructure::db_tx::LogErr;
use sqlx::PgPool;

use crate::domain::services::jp_pint_mapper;
use crate::infrastructure::peppol_client::PeppolClient;
use crate::infrastructure::peppol_importer::{self, PeppolInboundPayload};
use crate::infrastructure::repositories::{billing_repo, client_repo, company_info_repo, order_repo, partner_repo, peppol_repo};

fn err_json(msg: impl Into<String>) -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "success": false, "error": msg.into() }))
}

/// POST /api/v1/invoices/{id}/peppol/send
pub async fn send_invoice(State(pool): State<PgPool>, Path(id): Path<i64>) -> impl IntoResponse {
    if let Some(r) = super::invoices::current_guard(&pool, id).await {
        return r;
    }
    let invoice = match billing_repo::find_invoice(&pool, id).await {
        Ok(Some(inv)) => inv,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, err_json("請求書が見つかりません")).into_response(),
        Err(e) => { tracing::error!("peppol send_invoice: find_invoice failed: {:?}", e); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("内部エラー")).into_response(); }
    };
    let items = billing_repo::list_invoice_items(&pool, id).await.log_err().unwrap_or_default();
    let client = match client_repo::find_by_id(&pool, invoice.client_id).await {
        Ok(Some(c)) => c,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("クライアント情報が見つかりません")).into_response(),
    };
    let company = match company_info_repo::get_company_info_basic(&pool).await {
        Ok(Some(c)) => c,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("自社情報が未設定です")).into_response(),
    };

    let own_participant_id = PeppolClient::own_participant_id();
    let payload = jp_pint_mapper::map_sales_invoice(&invoice, &items, &client, &company, &own_participant_id);

    let client_res = PeppolClient::from_env();
    let result = match &client_res {
        Ok(c) => c.send_invoice(&payload).await,
        Err(e) => Err(match e { crate::infrastructure::peppol_client::PeppolError::NotConfigured => crate::infrastructure::peppol_client::PeppolError::NotConfigured, other => crate::infrastructure::peppol_client::PeppolError::ApiError(other.to_string()) }),
    };

    let request_json = serde_json::to_value(&payload).ok();
    match result {
        Ok(resp) => {
            if let Err(e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "INVOICE", "t_billing_invoice", &id.to_string(),
                &resp.message_id, &client.peppol_participant_id, "SENT",
                request_json, serde_json::to_value(&resp).ok(), "",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 invoice_id={} message_id={} | {}", id, resp.message_id, e);
            }
            axum::Json(serde_json::json!({ "success": true, "message_id": resp.message_id, "status": resp.status })).into_response()
        }
        Err(e) => {
            tracing::error!("[Peppol] 処理=請求書送信 影響=送信失敗 invoice_id={} | {}", id, e);
            if let Err(log_e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "INVOICE", "t_billing_invoice", &id.to_string(),
                "", &client.peppol_participant_id, "FAILED",
                request_json, None, "送信失敗",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 invoice_id={} | {}", id, log_e);
            }
            (axum::http::StatusCode::BAD_GATEWAY, err_json("Peppol送信に失敗しました")).into_response()
        }
    }
}

/// POST /api/v1/notices/{id}/peppol/send
pub async fn send_notice(State(pool): State<PgPool>, Path(id): Path<String>) -> impl IntoResponse {
    if let Some(r) = super::notices::current_guard(&pool, &id).await {
        return r;
    }
    let notice = match order_repo::find_payment_notice(&pool, &id).await {
        Ok(Some(n)) => n,
        Ok(None) => return (axum::http::StatusCode::NOT_FOUND, err_json("支払通知書が見つかりません")).into_response(),
        Err(e) => { tracing::error!("peppol send_notice: find_payment_notice failed: {:?}", e); return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("内部エラー")).into_response(); }
    };
    let items = order_repo::list_payment_notice_items(&pool, &id).await.log_err().unwrap_or_default();

    let mut item_names = Vec::with_capacity(items.len());
    for item in &items {
        let name = order_repo::find_engineer_name_for_partner_contract(&pool, item.partner_contract_id)
            .await.log_err().ok().flatten().unwrap_or_default();
        item_names.push(name);
    }

    let partner = match partner_repo::find_by_id(&pool, &notice.partner_id).await {
        Ok(Some(p)) => p,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("パートナー情報が見つかりません")).into_response(),
    };
    let company = match company_info_repo::get_company_info_basic(&pool).await {
        Ok(Some(c)) => c,
        _ => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, err_json("自社情報が未設定です")).into_response(),
    };

    let own_participant_id = PeppolClient::own_participant_id();
    let payload = jp_pint_mapper::map_self_billing_notice(&notice, &items, &item_names, &partner, &company, &own_participant_id);

    let client_res = PeppolClient::from_env();
    let result = match &client_res {
        Ok(c) => c.send_self_billing(&payload).await,
        Err(e) => Err(match e { crate::infrastructure::peppol_client::PeppolError::NotConfigured => crate::infrastructure::peppol_client::PeppolError::NotConfigured, other => crate::infrastructure::peppol_client::PeppolError::ApiError(other.to_string()) }),
    };

    let request_json = serde_json::to_value(&payload).ok();
    match result {
        Ok(resp) => {
            if let Err(e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "SELF_BILLING", "t_payment_notice", &id,
                &resp.message_id, &partner.peppol_participant_id, "SENT",
                request_json, serde_json::to_value(&resp).ok(), "",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 notice_id={} message_id={} | {}", id, resp.message_id, e);
            }
            axum::Json(serde_json::json!({ "success": true, "message_id": resp.message_id, "status": resp.status })).into_response()
        }
        Err(e) => {
            tracing::error!("[Peppol] 処理=支払通知書送信 影響=送信失敗 notice_id={} | {}", id, e);
            if let Err(log_e) = peppol_repo::insert_transmission(
                &pool, "OUTBOUND", "SELF_BILLING", "t_payment_notice", &id,
                "", &partner.peppol_participant_id, "FAILED",
                request_json, None, "送信失敗",
            ).await {
                tracing::error!("[Peppol] 処理=送受信ログ書込 影響=監査証跡未記録 notice_id={} | {}", id, log_e);
            }
            (axum::http::StatusCode::BAD_GATEWAY, err_json("Peppol送信に失敗しました")).into_response()
        }
    }
}

/// Peppol 受信 Webhook の認証判定結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeppolAuth {
    /// 秘密が一致した
    Allow,
    /// 秘密が空で、開発用（ENV_NAME=local / staging）のため認証を省略する
    AllowSkipped,
    /// 秘密が不一致、またはヘッダ無し（401）
    Unauthorized,
    /// 秘密が空で、省略が許されない環境（未設定・production・dr 等）（503）
    Misconfigured,
}

/// Peppol 受信 Webhook の認証判定（純関数）。
/// - 秘密が空: `env_name` が `local` / `staging` と明示されたときだけ `AllowSkipped`。それ以外は `Misconfigured`
/// - 秘密あり: `provided` と定数時間で比較する。一致で `Allow`、不一致・ヘッダ無しは `Unauthorized`
pub fn peppol_auth_decision(
    env_name: Option<&str>,
    secret: &str,
    provided: Option<&str>,
) -> PeppolAuth {
    if secret.is_empty() {
        return match env_name {
            Some("local" | "staging") => PeppolAuth::AllowSkipped,
            _ => PeppolAuth::Misconfigured,
        };
    }
    match provided {
        Some(p) if secrets_match(p.as_bytes(), secret.as_bytes()) => PeppolAuth::Allow,
        _ => PeppolAuth::Unauthorized,
    }
}

/// 長さの違いも含めて全バイトを比較する。両方を最大長まで 0 埋めして比較し、長さの一致も同時に確かめる。
/// 途中で打ち切らないため、一致箇所の位置や長さの差による応答時間の差を抑える。
fn secrets_match(provided: &[u8], secret: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    let n = provided.len().max(secret.len());
    let pad = |s: &[u8]| -> Vec<u8> {
        let mut v = vec![0u8; n];
        for (dst, src) in v.iter_mut().zip(s) {
            *dst = *src;
        }
        v
    };
    let len_eq = (provided.len() as u64).ct_eq(&(secret.len() as u64));
    let bytes_eq = pad(provided).ct_eq(&pad(secret));
    bool::from(len_eq & bytes_eq)
}

/// POST /api/v1/peppol/inbound — Peppolプロバイダーからの受信Webhook（雛形）
/// `document_type` フィールドで文書種別を判定し処理を振り分ける。
/// - "ORDER": 発注書受信 → t_received_order新規作成
/// - それ以外（未指定含む）: 自己発行請求書（セルフビリング）受信 → t_billing_invoiceと自動突合
///
/// 外部（Peppolプロバイダー）からのサーバー間呼び出しのため、ログインセッションではなく
/// 共有シークレット（PEPPOL_WEBHOOK_SECRET + X-Peppol-Webhook-Secret ヘッダ）で認証する
/// （認証鍵は PEPPOL_WEBHOOK_SECRET。GAS 連携の Webhook は撤去済みで、そちらの鍵とは無関係）。
/// 既定で拒否する（本リポの docs/開発標準書.md §3.5 #3「認証・鍵・設定欠落」）。
/// 秘密が空で認証を省略してよいのは ENV_NAME が `local` / `staging` と明示されたときだけ
/// （ENV_NAME 未設定・production・dr 等は 503）。判定は `peppol_auth_decision` が行う。
pub async fn inbound(State(pool): State<PgPool>, headers: HeaderMap, Json(body): Json<serde_json::Value>) -> impl IntoResponse {
    let env_raw = std::env::var("ENV_NAME").ok();
    let env_name = env_raw.as_deref().map(str::trim);
    let secret = std::env::var("PEPPOL_WEBHOOK_SECRET").unwrap_or_default();
    let provided = headers
        .get("X-Peppol-Webhook-Secret")
        .and_then(|v| v.to_str().ok());
    match peppol_auth_decision(env_name, &secret, provided) {
        PeppolAuth::Allow => {}
        PeppolAuth::AllowSkipped => {
            tracing::warn!(
                "[Peppol] 処理=受信Webhook認証 結果=省略 影響=開発用に認証を省略（ENV_NAME={}）",
                env_name.unwrap_or("")
            );
        }
        PeppolAuth::Unauthorized => {
            tracing::warn!("[Peppol] 処理=受信Webhook認証 結果=失敗 影響=受信拒否（401）");
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                err_json("Unauthorized"),
            )
                .into_response();
        }
        PeppolAuth::Misconfigured => {
            tracing::error!(
                "[Peppol] 処理=受信Webhook認証 結果=失敗 影響=秘密未設定かつ省略不可の環境のため受信拒否（ENV_NAME={}）",
                env_name.unwrap_or("未設定")
            );
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                err_json("サービス設定不備"),
            )
                .into_response();
        }
    }

    let document_type = body.get("document_type").and_then(|v| v.as_str()).unwrap_or("");

    if document_type == "ORDER" {
        let payload: crate::infrastructure::peppol_importer::PeppolInboundOrderPayload = match serde_json::from_value(body.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("[Peppol] 処理=受信発注書解析 影響=受信処理失敗 | {}", e);
                return (axum::http::StatusCode::BAD_REQUEST, err_json("ペイロード形式が無効です")).into_response();
            }
        };
        return match peppol_importer::import_inbound_order(&pool, &payload, body).await {
            Ok(result) => axum::Json(serde_json::json!({
                "success": true, "created": result.created, "received_order_id": result.received_order_id
            })).into_response(),
            Err(e) => {
                tracing::error!("[Peppol] 処理=受信発注書処理 影響=受信処理失敗 | {}", e);
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("受信処理に失敗しました")).into_response()
            }
        };
    }

    let payload: PeppolInboundPayload = match serde_json::from_value(body.clone()) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("[Peppol] 処理=受信文書解析 影響=受信処理失敗 | {}", e);
            return (axum::http::StatusCode::BAD_REQUEST, err_json("ペイロード形式が無効です")).into_response();
        }
    };

    match peppol_importer::import_inbound_document(&pool, &payload, body).await {
        Ok(result) => axum::Json(serde_json::json!({
            "success": true, "matched": result.matched, "billing_invoice_id": result.billing_invoice_id
        })).into_response(),
        Err(e) => {
            tracing::error!("[Peppol] 処理=受信文書処理 影響=受信処理失敗 | {}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("受信処理に失敗しました")).into_response()
        }
    }
}

/// GET /api/v1/peppol/status — Peppol の接続設定(API の URL・キー)が済んでいるか。
/// 未設定の環境では、画面の「Peppol送信」ボタンを押せなくするために使う(値そのものは返さない)
pub async fn status() -> impl IntoResponse {
    axum::Json(serde_json::json!({ "configured": PeppolClient::from_env().is_ok() }))
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct TransmissionFilter {
    pub status: Option<String>,
}

/// GET /api/v1/peppol/transmissions?status=all — 既定は未処理（FAILED/PENDING/UNMATCHED）のみ
pub async fn list_transmissions(State(pool): State<PgPool>, Query(filter): Query<TransmissionFilter>) -> impl IntoResponse {
    let result = if filter.status.as_deref() == Some("all") {
        peppol_repo::list_all(&pool).await
    } else {
        peppol_repo::list_actionable(&pool).await
    };

    match result {
        Ok(rows) => axum::Json(rows).into_response(),
        Err(e) => {
            tracing::error!("peppol list_transmissions failed: {:?}", e);
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_json("内部エラー")).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{peppol_auth_decision, secrets_match, PeppolAuth};

    #[test]
    fn empty_secret_without_env_name_is_misconfigured() {
        assert_eq!(
            peppol_auth_decision(None, "", None),
            PeppolAuth::Misconfigured
        );
        assert_eq!(
            peppol_auth_decision(None, "", Some("x")),
            PeppolAuth::Misconfigured
        );
    }

    #[test]
    fn empty_secret_in_production_is_misconfigured() {
        assert_eq!(
            peppol_auth_decision(Some("production"), "", None),
            PeppolAuth::Misconfigured
        );
        assert_eq!(
            peppol_auth_decision(Some("production"), "", Some("")),
            PeppolAuth::Misconfigured
        );
    }

    #[test]
    fn empty_secret_in_dr_is_misconfigured() {
        assert_eq!(
            peppol_auth_decision(Some("dr"), "", None),
            PeppolAuth::Misconfigured
        );
    }

    #[test]
    fn empty_secret_in_unknown_or_blank_env_is_misconfigured() {
        assert_eq!(
            peppol_auth_decision(Some("prod"), "", None),
            PeppolAuth::Misconfigured
        );
        assert_eq!(
            peppol_auth_decision(Some(""), "", None),
            PeppolAuth::Misconfigured
        );
        // 大文字小文字は区別する（明示された値のみ省略を許す）
        assert_eq!(
            peppol_auth_decision(Some("LOCAL"), "", None),
            PeppolAuth::Misconfigured
        );
        assert_eq!(
            peppol_auth_decision(Some("Staging"), "", None),
            PeppolAuth::Misconfigured
        );
    }

    #[test]
    fn empty_secret_in_local_or_staging_is_skipped() {
        assert_eq!(
            peppol_auth_decision(Some("local"), "", None),
            PeppolAuth::AllowSkipped
        );
        assert_eq!(
            peppol_auth_decision(Some("staging"), "", None),
            PeppolAuth::AllowSkipped
        );
        assert_eq!(
            peppol_auth_decision(Some("local"), "", Some("anything")),
            PeppolAuth::AllowSkipped
        );
    }

    #[test]
    fn matching_secret_is_allowed_in_any_env() {
        for env in [
            None,
            Some("local"),
            Some("staging"),
            Some("production"),
            Some("dr"),
        ] {
            assert_eq!(
                peppol_auth_decision(env, "s3cret-value", Some("s3cret-value")),
                PeppolAuth::Allow
            );
        }
    }

    #[test]
    fn missing_header_is_unauthorized_when_secret_set() {
        for env in [None, Some("local"), Some("production")] {
            assert_eq!(
                peppol_auth_decision(env, "s3cret-value", None),
                PeppolAuth::Unauthorized
            );
        }
    }

    #[test]
    fn mismatched_secret_is_unauthorized() {
        let secret = "s3cret-value";
        // 同じ長さで1バイト違う / 先頭一致の短い値 / 末尾が余分な値 / 空文字
        assert_eq!(
            peppol_auth_decision(Some("production"), secret, Some("s3cret-valuE")),
            PeppolAuth::Unauthorized
        );
        assert_eq!(
            peppol_auth_decision(Some("production"), secret, Some("s3cret")),
            PeppolAuth::Unauthorized
        );
        assert_eq!(
            peppol_auth_decision(Some("production"), secret, Some("s3cret-value!")),
            PeppolAuth::Unauthorized
        );
        assert_eq!(
            peppol_auth_decision(Some("production"), secret, Some("")),
            PeppolAuth::Unauthorized
        );
        assert_eq!(
            peppol_auth_decision(Some("local"), secret, Some("wrong")),
            PeppolAuth::Unauthorized
        );
    }

    #[test]
    fn secrets_match_covers_length_and_content() {
        assert!(secrets_match(b"abc", b"abc"));
        assert!(secrets_match(b"", b""));
        assert!(!secrets_match(b"abc", b"abd"));
        assert!(!secrets_match(b"ab", b"abc"));
        assert!(!secrets_match(b"abc", b"ab"));
        assert!(!secrets_match(b"", b"abc"));
        assert!(!secrets_match(b"abc", b""));
        // 0 埋めで区別できないこと（末尾が NUL の値は別物）
        assert!(!secrets_match(b"abc\0", b"abc"));
    }
}
