/// infrastructure/peppol_importer.rs — Peppol受信文書の取込・自動照合
///
/// billing_importer.rs（EDI-OASISからの請求データ取込）と同じ構造。
/// 受信した自己発行請求書（クライアントが自社に代わって発行したセルフビリング文書）を
/// t_peppol_transmissionにRECEIVEDとして記録し、送信元 + 対象月 + 金額でt_billing_invoiceを
/// 検索して自動突合する。完全一致しない場合はUNMATCHEDのまま残し、確定処理は行わない（雛形）。

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;

use crate::infrastructure::repositories::{order_repo, peppol_repo};

#[derive(Debug, Clone, Deserialize, Default)]
pub struct PeppolInboundPayload {
    pub peppol_message_id: String,
    pub sender_participant_id: String,
    #[serde(default)]
    pub sender_registration_no: String,
    #[serde(default)]
    pub total_amount: i32,
    #[serde(default)]
    pub target_month: Option<NaiveDate>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MatchResult {
    pub matched: bool,
    pub billing_invoice_id: Option<i64>,
}

/// 受信文書を記録し、自社の売掛金データ（t_billing_invoice）との自動突合を試みる
pub async fn import_inbound_document(
    pool: &PgPool,
    payload: &PeppolInboundPayload,
    raw: serde_json::Value,
) -> Result<MatchResult, String> {
    let log_id = peppol_repo::insert_transmission(
        pool, "INBOUND", "SELF_BILLING", "", "",
        &payload.peppol_message_id, &payload.sender_participant_id, "RECEIVED",
        Some(raw), None, "",
    ).await.map_err(|e| format!("t_peppol_transmission記録失敗: {e}"))?;

    // 送信元を registration_no または peppol_participant_id でクライアント特定
    let client_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client WHERE (registration_no = $1 AND $1 != '') OR (peppol_participant_id = $2 AND $2 != '') LIMIT 1",
    )
    .bind(&payload.sender_registration_no)
    .bind(&payload.sender_participant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("クライアント検索失敗: {e}"))?;

    let Some(client_id) = client_id else {
        tracing::warn!("[PeppolImporter] 送信元クライアント特定不可: participant_id={}", payload.sender_participant_id);
        return Ok(MatchResult { matched: false, billing_invoice_id: None });
    };

    let matched_id: Option<i64> = match payload.target_month {
        Some(target_month) => sqlx::query_scalar(
            "SELECT id FROM t_billing_invoice WHERE client_id = $1 AND target_month = $2 AND total = $3 LIMIT 1",
        )
        .bind(client_id)
        .bind(target_month)
        .bind(payload.total_amount)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("t_billing_invoice照合失敗: {e}"))?,
        None => None,
    };

    match matched_id {
        Some(invoice_id) => {
            peppol_repo::mark_matched(pool, log_id, "t_billing_invoice", &invoice_id.to_string())
                .await
                .map_err(|e| format!("突合状態更新失敗: {e}"))?;
            tracing::info!("[PeppolImporter] 自動突合成功: invoice_id={}", invoice_id);
            Ok(MatchResult { matched: true, billing_invoice_id: Some(invoice_id) })
        }
        None => {
            tracing::warn!("[PeppolImporter] 自動突合不可（UNMATCHEDのまま保持）: client_id={}", client_id);
            Ok(MatchResult { matched: false, billing_invoice_id: None })
        }
    }
}

/// 受信した発注書（Peppol Order）の明細1行
#[derive(Debug, Clone, Deserialize)]
pub struct PeppolInboundOrderLine {
    pub item_name: String,
    #[serde(default)]
    pub quantity: Decimal,
    #[serde(default)]
    pub unit_price: i32,
    #[serde(default)]
    pub line_amount: i32,
}

/// 受信した発注書（Peppol Order）のペイロード
#[derive(Debug, Clone, Deserialize)]
pub struct PeppolInboundOrderPayload {
    pub peppol_message_id: String,
    pub sender_participant_id: String,
    #[serde(default)]
    pub sender_registration_no: String,
    pub work_start: NaiveDate,
    pub work_end: NaiveDate,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub lines: Vec<PeppolInboundOrderLine>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ImportOrderResult {
    pub created: bool,
    pub received_order_id: Option<i64>,
}

/// 受信した発注書（クライアントからのOrder文書）を取り込み、t_received_orderを新規作成する。
/// 送信元がm_clientに登録されていない場合は作成せずFAILEDとして記録する。
pub async fn import_inbound_order(
    pool: &PgPool,
    payload: &PeppolInboundOrderPayload,
    raw: serde_json::Value,
) -> Result<ImportOrderResult, String> {
    let log_id = peppol_repo::insert_transmission(
        pool, "INBOUND", "ORDER", "", "",
        &payload.peppol_message_id, &payload.sender_participant_id, "RECEIVED",
        Some(raw), None, "",
    ).await.map_err(|e| format!("t_peppol_transmission記録失敗: {e}"))?;

    let client_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client WHERE (registration_no = $1 AND $1 != '') OR (peppol_participant_id = $2 AND $2 != '') LIMIT 1",
    )
    .bind(&payload.sender_registration_no)
    .bind(&payload.sender_participant_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("クライアント検索失敗: {e}"))?;

    let Some(client_id) = client_id else {
        tracing::warn!("[PeppolImporter] 発注書の送信元クライアント特定不可: participant_id={}", payload.sender_participant_id);
        let _ = sqlx::query("UPDATE t_peppol_transmission SET status = 'FAILED', error_message = '送信元クライアントが未登録です（registration_no / peppol_participant_id 不一致）' WHERE id = $1")
            .bind(log_id)
            .execute(pool)
            .await;
        return Ok(ImportOrderResult { created: false, received_order_id: None });
    };

    let target_month = NaiveDate::from_ymd_opt(payload.work_start.year(), payload.work_start.month(), 1)
        .unwrap_or(payload.work_start);
    let month_str = target_month.format("%Y%m").to_string();

    let mut tx = pool.begin().await.map_err(|e| format!("TX error: {e}"))?;

    let existing_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM t_received_order WHERE received_order_no LIKE $1")
        .bind(format!("RO-{}-%", month_str))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| format!("採番カウント失敗: {e}"))?;
    let received_order_no = format!("RO-{}-{:03}", month_str, existing_count + 1);

    let order_id = order_repo::insert_received_order_basic(
        &mut tx, &received_order_no, client_id, target_month,
        payload.work_start, payload.work_end, &payload.project_name,
    ).await.map_err(|e| format!("t_received_order作成失敗: {e}"))?;

    for line in &payload.lines {
        order_repo::insert_received_order_item_minimal(
            &mut tx, order_id, &line.item_name, line.unit_price, line.quantity, line.quantity, line.line_amount,
        ).await.map_err(|e| format!("t_received_order_item作成失敗: {e}"))?;
    }

    tx.commit().await.map_err(|e| format!("Commit error: {e}"))?;

    peppol_repo::mark_matched(pool, log_id, "t_received_order", &order_id.to_string())
        .await
        .map_err(|e| format!("突合状態更新失敗: {e}"))?;

    tracing::info!("[PeppolImporter] 発注書受信→受注登録: order_id={} no={}", order_id, received_order_no);
    Ok(ImportOrderResult { created: true, received_order_id: Some(order_id) })
}
