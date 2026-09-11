//! 稼働報告アップロード／確定の共通ロジック（admin / partner 共用）
//!
//! ルートと契約解決・初期ステータス（PARSED vs UPLOADED）はハンドラ側に残し、
//! multipart 抽出・プレビュー JSON・月正規化・受注注文書確認・upsert を共通化する。

use axum::extract::Multipart;
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::domain::services::excel_parser::{auto_detect_and_parse, TimesheetParseResult};
use crate::domain::value_objects::SettlementTerms;
use crate::domain::models::actionable_error::ActionBlocker;
use crate::infrastructure::repositories::{order_repo, timesheet_repo};

/// アップロード／確定処理の共通エラー
#[derive(Debug)]
pub enum TimesheetUploadError {
    NoFile,
    ReadFailed,
    Parse(String),
    ContractNotFound { worker_name: String },
    ContractAmbiguous {
        worker_name: String,
        month_label: String,
        count: i64,
    },
    ContractMonthMismatch {
        worker_name: String,
        month_label: String,
    },
    ReceivedOrderMissing {
        worker_name: String,
        month_label: String,
    },
    ReceivedOrderCheckFailed,
    SaveFailed(String),
}

impl TimesheetUploadError {
    pub fn to_json(&self) -> Value {
        let (error_msg, blockers) = match self {
            Self::NoFile => (
                "ファイルが選択されていません".to_string(),
                vec![],
            ),
            Self::ReadFailed => (
                "ファイル読取に失敗しました".to_string(),
                vec![],
            ),
            Self::Parse(err) => (
                err.clone(),
                vec![],
            ),
            Self::ContractNotFound { worker_name } => {
                let blocker = ActionBlocker {
                    subject: worker_name.clone(),
                    context: String::new(),
                    reason: "受注契約が見つかりません".to_string(),
                    suggestion: "受注契約を登録してから再度お試しください。".to_string(),
                    link_path: "/client-contracts".to_string(),
                    link_label: "受注契約一覧を開く".to_string(),
                    code: "contract_not_found".to_string(),
                };
                (
                    format!("作業者「{}」の受注契約が見つかりません。受注契約を登録してから再度お試しください。", worker_name),
                    vec![blocker],
                )
            },
            Self::ContractAmbiguous {
                worker_name,
                month_label,
                count,
            } => {
                let blocker = ActionBlocker {
                    subject: worker_name.clone(),
                    context: month_label.clone(),
                    reason: format!("受注契約が{}件あり特定できません", count),
                    suggestion: "アップロード時に契約を明示選択するか、契約期間が重ならないよう整理してください。".to_string(),
                    link_path: "/client-contracts".to_string(),
                    link_label: "受注契約一覧を開く".to_string(),
                    code: "contract_ambiguous".to_string(),
                };
                (
                    format!(
                        "作業者「{}」の{}分の受注契約が{}件あり特定できません。アップロード時に契約を明示選択するか、契約期間が重ならないよう整理してください。",
                        worker_name, month_label, count
                    ),
                    vec![blocker],
                )
            },
            Self::ContractMonthMismatch {
                worker_name,
                month_label,
            } => {
                let blocker = ActionBlocker {
                    subject: worker_name.clone(),
                    context: month_label.clone(),
                    reason: "契約期間が対象月と重なりません".to_string(),
                    suggestion: "対象月に有効な契約を選んでください。".to_string(),
                    link_path: "/client-contracts".to_string(),
                    link_label: "受注契約一覧を開く".to_string(),
                    code: "contract_month_mismatch".to_string(),
                };
                (
                    format!(
                        "指定した受注契約は作業者「{}」の{}分の期間と重なりません。対象月に有効な契約を選んでください。",
                        worker_name, month_label
                    ),
                    vec![blocker],
                )
            },
            Self::ReceivedOrderMissing {
                worker_name,
                month_label,
            } => {
                let blocker = ActionBlocker {
                    subject: worker_name.clone(),
                    context: month_label.clone(),
                    reason: "受注書が見つかりません（技術者・案件・クライアントで不一致）".to_string(),
                    suggestion: "案件のEDI別名を設定するか、受注書詳細で受注契約を紐付けてから再度お試しください。".to_string(),
                    link_path: "/received-orders".to_string(),
                    link_label: "受注一覧を開く".to_string(),
                    code: "received_order_missing".to_string(),
                };
                (
                    format!(
                        "作業者「{}」の{}分の受注書が見つかりません（技術者・案件・クライアントで不一致）。案件のEDI別名を設定するか、受注書詳細で受注契約を紐付けてから再度お試しください。",
                        worker_name, month_label
                    ),
                    vec![blocker],
                )
            },
            Self::ReceivedOrderCheckFailed => (
                "受注書の確認中にエラーが発生しました".to_string(),
                vec![ActionBlocker {
                    subject: String::new(),
                    context: String::new(),
                    reason: "受注書の確認中にエラーが発生しました".to_string(),
                    suggestion: "しばらく待ってから再試行するか、管理者に連絡してください。".to_string(),
                    link_path: "/received-orders".to_string(),
                    link_label: "受注一覧を開く".to_string(),
                    code: "received_order_check_failed".to_string(),
                }],
            ),
            Self::SaveFailed(msg) => (msg.clone(), vec![]),
        };

        json!({
            "status": "error",
            "error": error_msg,
            "blockers": blockers,
        })
    }
}

/// multipart から `file` フィールドを抽出する。
/// ファイル名が欠落している場合は空文字（`unknown.xlsx` 等へのフォールバックはしない）。
pub async fn extract_upload_file(
    multipart: &mut Multipart,
) -> Result<(Vec<u8>, String), TimesheetUploadError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::new();

    while let Ok(Some(field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            // 欠落時に .xlsx を仮置きしない（PDFがExcel経路に落ちて EOCD エラーになるため）
            original_filename = field.file_name().unwrap_or("").to_string();
            match field.bytes().await {
                Ok(b) => file_bytes = Some(b.to_vec()),
                Err(e) => {
                    tracing::error!("Multipartバイト読取エラー: {:?}", e);
                    return Err(TimesheetUploadError::ReadFailed);
                }
            }
        }
    }

    match file_bytes {
        Some(bytes) => Ok((bytes, original_filename)),
        None => Err(TimesheetUploadError::NoFile),
    }
}

/// confirm 用: `file` と任意の `contract_id` を抽出する。
pub async fn extract_confirm_fields(
    multipart: &mut Multipart,
) -> Result<(Vec<u8>, String, Option<i64>), TimesheetUploadError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::new();
    let mut contract_id: Option<i64> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "contract_id" => {
                if let Ok(text) = field.text().await {
                    contract_id = text.parse().ok();
                }
            }
            "file" => {
                original_filename = field.file_name().unwrap_or("").to_string();
                match field.bytes().await {
                    Ok(b) => file_bytes = Some(b.to_vec()),
                    Err(e) => {
                        tracing::error!("Multipartバイト読取エラー: {:?}", e);
                        return Err(TimesheetUploadError::ReadFailed);
                    }
                }
            }
            _ => {}
        }
    }

    match file_bytes {
        Some(bytes) => Ok((bytes, original_filename, contract_id)),
        None => Err(TimesheetUploadError::NoFile),
    }
}

/// 受注契約の精算条件を取得する。
pub async fn settlement_terms_for_client_contract(
    pool: &PgPool,
    contract_id: i64,
) -> Result<SettlementTerms, TimesheetUploadError> {
    let contract = order_repo::find_client_contract_for_order(pool, contract_id)
        .await
        .map_err(|e| TimesheetUploadError::SaveFailed(format!("契約取得エラー: {}", e)))?
        .ok_or_else(|| {
            TimesheetUploadError::SaveFailed(format!("契約 id={} が見つかりません", contract_id))
        })?;

    Ok(SettlementTerms {
        lower_limit_hours: contract.lower_limit_hours,
        upper_limit_hours: contract.upper_limit_hours,
        fixed_hours: contract.fixed_hours,
        deduction_rate: contract.deduction_rate,
        overtime_rate: contract.overtime_rate,
    })
}

/// 解析結果の残業時間を精算超過時間に差し替える（法定残業・深夜・休日は使わない）。
pub fn apply_settlement_hours(result: &mut TimesheetParseResult, terms: &SettlementTerms) {
    result.overtime_hours = terms.excess_hours(result.total_hours);
    result.night_hours = Decimal::ZERO;
    result.holiday_hours = Decimal::ZERO;
}

/// 受注契約に基づき精算超過時間を解析結果へ反映する。
pub async fn apply_settlement_hours_for_contract(
    pool: &PgPool,
    contract_id: i64,
    result: &mut TimesheetParseResult,
) -> Result<(), TimesheetUploadError> {
    let terms = settlement_terms_for_client_contract(pool, contract_id).await?;
    apply_settlement_hours(result, &terms);
    Ok(())
}

/// admin プレビュー用: 契約を解決し精算超過時間を反映する。
pub async fn prepare_admin_preview_result(
    pool: &PgPool,
    mut result: TimesheetParseResult,
    explicit_contract_id: Option<i64>,
) -> Result<TimesheetParseResult, TimesheetUploadError> {
    let target_month = normalize_target_month(&result);
    let contract_id = resolve_admin_contract_id(
        pool,
        &result.worker_name,
        target_month,
        explicit_contract_id,
    )
    .await?;
    apply_settlement_hours_for_contract(pool, contract_id, &mut result).await?;
    Ok(result)
}

/// partner プレビュー用: 契約を解決し精算超過時間を反映する。
pub async fn prepare_partner_preview_result(
    pool: &PgPool,
    partner_id: &str,
    mut result: TimesheetParseResult,
    explicit_pk: i64,
) -> Result<TimesheetParseResult, TimesheetUploadError> {
    let target_month = normalize_target_month(&result);
    let contract_id = resolve_partner_contract_id(
        pool,
        partner_id,
        &result.worker_name,
        target_month,
        explicit_pk,
    )
    .await?;
    apply_settlement_hours_for_contract(pool, contract_id, &mut result).await?;
    Ok(result)
}

/// 解析結果から SPA 向けプレビュー JSON を組み立てる。
/// - `target_month`: `"YYYY-MM"`（空なら空文字）
/// - `alerts`: 文字列配列（`detail`）
pub fn build_preview_json(
    result: &TimesheetParseResult,
    original_filename: &str,
) -> Value {
    let target_month = result
        .target_month
        .map(|d| d.format("%Y-%m").to_string())
        .unwrap_or_default();
    let alerts: Vec<String> = result.alerts.iter().map(|a| a.detail.clone()).collect();

    json!({
        "worker_name": result.worker_name,
        "target_month": target_month,
        "total_hours": result.total_hours.to_string(),
        "work_days": result.work_days,
        "overtime_hours": result.overtime_hours.to_string(),
        "night_hours": result.night_hours.to_string(),
        "holiday_hours": result.holiday_hours.to_string(),
        "sheet_name": result.sheet_name,
        "original_filename": original_filename,
        "daily_data": result.daily_data,
        "alerts": alerts,
        "has_times": result.has_times,
    })
}

/// ファイルを解析し、パースエラーなら Err。
pub fn parse_upload(
    file_bytes: &[u8],
    original_filename: &str,
) -> Result<TimesheetParseResult, TimesheetUploadError> {
    let result = auto_detect_and_parse(file_bytes, original_filename);
    if let Some(err) = &result.error {
        return Err(TimesheetUploadError::Parse(err.clone()));
    }
    Ok(result)
}

/// 対象月を月初に正規化する（受注注文書の target_month と突合するため）。
pub fn normalize_target_month(result: &TimesheetParseResult) -> NaiveDate {
    result
        .target_month
        .unwrap_or_else(|| chrono::Utc::now().date_naive())
        .with_day(1)
        .unwrap_or_else(|| {
            chrono::Utc::now()
                .date_naive()
                .with_day(1)
                .unwrap_or_default()
        })
}

/// 受注書（t_received_order）の存在確認。
/// 契約ID直結、または技術者・対象月・クライアント・案件名の一致で可。
pub async fn ensure_received_order(
    pool: &PgPool,
    contract_id: i64,
    target_month: NaiveDate,
    worker_name: &str,
) -> Result<(), TimesheetUploadError> {
    let month_label = target_month.format("%Y年%m月").to_string();
    match timesheet_repo::exists_received_order_for_contract_month(pool, contract_id, target_month)
        .await
    {
        Ok(true) => Ok(()),
        Ok(false) => {
            tracing::error!(
                "受注注文書なし: worker={}, contract_id={}, month={}",
                worker_name,
                contract_id,
                target_month
            );
            Err(TimesheetUploadError::ReceivedOrderMissing {
                worker_name: worker_name.to_string(),
                month_label,
            })
        }
        Err(e) => {
            tracing::error!("受注注文書確認エラー: {}", e);
            Err(TimesheetUploadError::ReceivedOrderCheckFailed)
        }
    }
}

/// 解析結果を upsert する（status は呼び出し側で指定: PARSED / UPLOADED）。
/// 保存成功後、対応する発注・受注を `REPORT_RECEIVED`（報告受領）へ進める。
pub async fn upsert_parsed_timesheet(
    pool: &PgPool,
    contract_id: i64,
    target_month: NaiveDate,
    result: &TimesheetParseResult,
    original_filename: &str,
    status: &str,
) -> Result<(), TimesheetUploadError> {
    let mut result = result.clone();
    apply_settlement_hours_for_contract(pool, contract_id, &mut result).await?;

    let daily_json = serde_json::to_value(&result.daily_data).unwrap_or_default();
    let alerts_json = serde_json::to_value(&result.alerts).unwrap_or_default();

    timesheet_repo::upsert_monthly_timesheet_upload(
        pool,
        contract_id,
        target_month,
        result.total_hours,
        result.work_days,
        result.overtime_hours,
        result.night_hours,
        result.holiday_hours,
        original_filename,
        &daily_json,
        &alerts_json,
        status,
    )
    .await
    .map_err(|e| TimesheetUploadError::SaveFailed(format!("保存エラー: {}", e)))?;

    advance_orders_on_timesheet_receipt(pool, contract_id, target_month).await;
    Ok(())
}

/// 稼働報告のシステム受領に伴い発注/受注を REPORT_RECEIVED へ進める。
/// ステータス更新失敗は稼働報告登録自体を失敗させない（ログのみ）。
pub async fn advance_orders_on_timesheet_receipt(
    pool: &PgPool,
    client_contract_id: i64,
    target_month: NaiveDate,
) {
    match order_repo::advance_to_report_received_for_timesheet(
        pool,
        client_contract_id,
        target_month,
    )
    .await
    {
        Ok((po_n, ro_n)) => {
            if po_n > 0 || ro_n > 0 {
                tracing::info!(
                    "報告受領ステータス更新: purchase_orders={}, received_orders={}, client_contract_id={}, target_month={}",
                    po_n,
                    ro_n,
                    client_contract_id,
                    target_month
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                "報告受領ステータス更新失敗（稼働報告は保存済み）: client_contract_id={}, target_month={}, err={}",
                client_contract_id,
                target_month,
                e
            );
        }
    }
}

/// admin: 作業者名 + 対象月から受注契約を解決する。
pub async fn resolve_admin_contract_id(
    pool: &PgPool,
    worker_name: &str,
    target_month: NaiveDate,
    explicit_contract_id: Option<i64>,
) -> Result<i64, TimesheetUploadError> {
    let month_label = format!("{}年{}月", target_month.year(), target_month.month());

    if let Some(cid) = explicit_contract_id {
        let covers = timesheet_repo::client_contract_covers_month(pool, cid, target_month)
            .await
            .unwrap_or(false);
        if !covers {
            return Err(TimesheetUploadError::ContractMonthMismatch {
                worker_name: worker_name.to_string(),
                month_label,
            });
        }
        return Ok(cid);
    }

    let worker = worker_name.replace('　', " ").replace(' ', "");
    match timesheet_repo::find_client_contract_id_by_engineer_name_month(pool, &worker, target_month)
        .await
        .unwrap_or(Ok(None))
    {
        Ok(Some(id)) => return Ok(id),
        Ok(None) => {}
        Err(count) => {
            return Err(TimesheetUploadError::ContractAmbiguous {
                worker_name: worker_name.to_string(),
                month_label,
                count,
            });
        }
    }

    tracing::error!("契約が見つかりません: worker_name={}, month={}", worker_name, month_label);
    match timesheet_repo::find_client_contract_id_by_engineer_name_like_month(
        pool,
        worker_name,
        target_month,
    )
    .await
    .unwrap_or(Ok(None))
    {
        Ok(Some(id)) => Ok(id),
        Ok(None) => {
            tracing::error!("契約解決失敗: {}", worker_name);
            Err(TimesheetUploadError::ContractNotFound {
                worker_name: worker_name.to_string(),
            })
        }
        Err(count) => Err(TimesheetUploadError::ContractAmbiguous {
            worker_name: worker_name.to_string(),
            month_label,
            count,
        }),
    }
}

/// partner: 自社スコープで受注契約を解決する。`explicit_pk > 0` ならそれを使う（対象月と重なること）。
pub async fn resolve_partner_contract_id(
    pool: &PgPool,
    partner_id: &str,
    worker_name: &str,
    target_month: NaiveDate,
    explicit_pk: i64,
) -> Result<i64, TimesheetUploadError> {
    let month_label = format!("{}年{}月", target_month.year(), target_month.month());

    if explicit_pk > 0 {
        let covers = timesheet_repo::client_contract_covers_month(pool, explicit_pk, target_month)
            .await
            .unwrap_or(false);
        if !covers {
            return Err(TimesheetUploadError::ContractMonthMismatch {
                worker_name: worker_name.to_string(),
                month_label,
            });
        }
        return Ok(explicit_pk);
    }

    let worker = worker_name.replace('　', " ").replace(' ', "");
    match timesheet_repo::find_client_contract_id_by_partner_worker_month(
        pool,
        partner_id,
        &worker,
        target_month,
    )
    .await
    .unwrap_or(Ok(None))
    {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(TimesheetUploadError::ContractNotFound {
            worker_name: worker_name.to_string(),
        }),
        Err(count) => Err(TimesheetUploadError::ContractAmbiguous {
            worker_name: worker_name.to_string(),
            month_label,
            count,
        }),
    }
}

/// プレビュー用 upload 応答 JSON。
pub fn ok_preview_response(result: &TimesheetParseResult, original_filename: &str) -> Value {
    json!({
        "status": "ok",
        "preview": build_preview_json(result, original_filename),
    })
}

/// 確定登録成功応答（admin 向けメッセージのみ）。
pub fn ok_confirm_response() -> Value {
    json!({
        "status": "ok",
        "message": "稼働報告を登録しました",
    })
}
