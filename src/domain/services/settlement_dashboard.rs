/// domain/services/settlement_dashboard.rs — 月次確定ダッシュボード ドメインサービス
///
/// ダッシュボード画面のビジネスロジックを集約。
/// - パートナー単位の支払通知書集約発行
/// - 上司承認判定
/// - プレビュー計算

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use sqlx::{PgPool, FromRow};

use crate::domain::models::partner_contract::ApprovalStatus;
use crate::domain::models::actionable_error::ActionBlocker;
use crate::infrastructure::repositories::settlement_repo;

/// ダッシュボード1行分のビューモデル（JOINクエリの結果）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SettlementRow {
    // 共通
    pub engineer_id: i64,
    pub engineer_name: String,
    pub partner_id: Option<String>,
    pub partner_name: Option<String>,
    pub client_name: String,
    pub client_id: i64,
    /// クライアント自身が保有するEDIシステムの種別（例: "EDI_OASIS"）。空文字ならEDIシステムなし。
    /// Sophia自身も広義にはEDIシステムだが、ここでは「先方が別途外部EDIシステム(OASIS等)を
    /// 保有しているか」を表す。保有している場合、請求書は先方のEDI経由で受け取る（Sophiaから
    /// 新規発行しない）
    pub client_edi_system_type: String,
    // 稼働報告
    pub timesheet_id: Option<i64>,
    pub total_hours: Option<Decimal>,
    pub timesheet_status: Option<String>,
    // 請求（売上）側
    pub client_contract_id: i64,
    pub billing_base_rate: i32,
    pub billing_settlement_type: String,
    pub billing_lower_limit: Decimal,
    pub billing_upper_limit: Decimal,
    pub billing_fixed_hours: Option<Decimal>,
    pub billing_deduction_rate: i32,
    pub billing_overtime_rate: i32,
    pub billing_effort: Decimal,
    /// クライアント契約の支払条件テキスト（例: 毎月末日締め翌月末日払い）。空ならデフォルト適用。
    pub billing_payment_terms: String,
    // 支払（原価）側 — 社員の場合はNULL
    pub partner_contract_id: Option<i64>,
    pub payment_base_rate: Option<i32>,
    pub payment_settlement_type: Option<String>,
    pub payment_lower_limit: Option<Decimal>,
    pub payment_upper_limit: Option<Decimal>,
    pub payment_fixed_hours: Option<Decimal>,
    pub payment_deduction_rate: Option<i32>,
    pub payment_overtime_rate: Option<i32>,
    pub payment_effort: Option<Decimal>,
    /// パートナー契約の支払条件テキスト（例: 毎月末日締め翌月末日払い）。空ならデフォルト適用。
    pub payment_condition: String,
    // 発行ステータス
    pub invoice_issued: Option<bool>,
    pub notice_issued: Option<bool>,
    /// 対象月の発注注文書（診断・準備状況表示用。最新1件）
    pub purchase_order_id: Option<String>,
    pub purchase_order_status: Option<String>,
    // プロジェクト別ダッシュボード用
    pub project_id: String,
    pub project_name: String,
    pub engineer_employee_code: String,
}

/// プレビュー計算済みの行
#[derive(Debug, Clone, Serialize)]
pub struct SettlementViewRow {
    pub row: SettlementRow,
    pub billing_amount: i32,
    pub payment_amount: i32,
    pub profit: i32,
    pub profit_rate: f64,
    pub profit_rate_display: String,
}

/// サマリー情報
#[derive(Debug, Clone, Serialize, Default)]
pub struct SettlementSummary {
    pub total_count: usize,
    pub total_billing: i64,
    pub total_payment: i64,
    pub total_profit: i64,
    pub avg_profit_rate: f64,
    pub avg_profit_rate_display: String,
    pub invoice_issued_count: usize,
    pub invoice_pending_count: usize,
    pub notice_issued_count: usize,
    pub notice_pending_count: usize,
    pub unapproved_count: usize,
}

/// フィルタ条件
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SettlementFilter {
    pub target_month: Option<String>,
    pub client_id: Option<i64>,
    pub partner_id: Option<String>,
    pub project_id: Option<String>,
    pub status: Option<String>,
}

/// JOINクエリで月次確定データを取得
pub async fn list_settlement_rows(
    pool: &PgPool,
    target_month: NaiveDate,
    filter: &SettlementFilter,
) -> anyhow::Result<Vec<SettlementRow>> {
    settlement_repo::list_settlement_rows(pool, target_month, filter)
        .await
        .map_err(|e| anyhow::anyhow!("settlement_dashboard: failed to fetch settlement rows: {}", e))
}

/// 精算計算プレビュー
pub fn calculate_preview(rows: Vec<SettlementRow>) -> (Vec<SettlementViewRow>, SettlementSummary) {
    use crate::domain::value_objects::SettlementTerms;

    let mut view_rows = Vec::new();
    let mut summary = SettlementSummary::default();

    for row in rows {
        let actual_hours = row.total_hours.unwrap_or(Decimal::ZERO);
        let ts_approved = row.timesheet_status.as_deref() == Some("APPROVED");

        // 請求金額（売上）
        let billing_amount = if ts_approved {
            let terms = SettlementTerms {
                lower_limit_hours: row.billing_lower_limit,
                upper_limit_hours: row.billing_upper_limit,
                fixed_hours: row.billing_fixed_hours,
                deduction_rate: row.billing_deduction_rate,
                overtime_rate: row.billing_overtime_rate,
            };
            terms.calculate_amount(row.billing_base_rate, row.billing_effort, actual_hours)
        } else {
            0
        };

        // 支払金額（原価）— パートナー契約がない社員は0
        let payment_amount = if ts_approved && row.partner_contract_id.is_some() {
            let terms = SettlementTerms {
                lower_limit_hours: row.payment_lower_limit.unwrap_or(Decimal::ZERO),
                upper_limit_hours: row.payment_upper_limit.unwrap_or(Decimal::ZERO),
                fixed_hours: row.payment_fixed_hours,
                deduction_rate: row.payment_deduction_rate.unwrap_or(0),
                overtime_rate: row.payment_overtime_rate.unwrap_or(0),
            };
            terms.calculate_amount(row.payment_base_rate.unwrap_or(0), row.payment_effort.unwrap_or(Decimal::ONE), actual_hours)
        } else {
            0
        };

        let profit = billing_amount - payment_amount;
        let profit_rate = if billing_amount > 0 {
            (profit as f64 / billing_amount as f64) * 100.0
        } else {
            0.0
        };

        // サマリー集計
        summary.total_count += 1;
        summary.total_billing += billing_amount as i64;
        summary.total_payment += payment_amount as i64;
        summary.total_profit += profit as i64;

        if row.invoice_issued.unwrap_or(false) {
            summary.invoice_issued_count += 1;
        } else {
            summary.invoice_pending_count += 1;
        }
        // 社員（パートナー契約なし）は支払通知不要なのでカウントしない
        if row.partner_contract_id.is_some() {
            if row.notice_issued.unwrap_or(false) {
                summary.notice_issued_count += 1;
            } else {
                summary.notice_pending_count += 1;
            }
        }
        if !ts_approved && row.timesheet_id.is_some() {
            summary.unapproved_count += 1;
        }
        if row.timesheet_id.is_none() {
            summary.unapproved_count += 1;
        }

        view_rows.push(SettlementViewRow {
            row,
            billing_amount,
            payment_amount,
            profit,
            profit_rate,
            profit_rate_display: format!("{:.1}", profit_rate),
        });
    }

    if summary.total_billing > 0 {
        summary.avg_profit_rate = (summary.total_profit as f64 / summary.total_billing as f64) * 100.0;
    }
    summary.avg_profit_rate_display = format!("{:.1}", summary.avg_profit_rate);

    (view_rows, summary)
}

/// 上司承認が必要かを判定
pub async fn get_approval_threshold(pool: &PgPool) -> i32 {
    settlement_repo::get_notice_approval_threshold(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(500_000)
}

/// トークン有効期限を取得（日数）
pub async fn get_token_expiry_days(pool: &PgPool) -> i32 {
    settlement_repo::get_token_expiry_days(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(14)
}

/// 支払通知書番号を自動採番する（PN-YYYYMM-001）
pub async fn generate_notice_id(pool: &PgPool, target_month: NaiveDate) -> String {
    let prefix = format!("PN-{}", target_month.format("%Y%m"));
    let max_seq: Option<String> = settlement_repo::max_notice_id_with_prefix(pool, &prefix)
        .await
        .ok()
        .flatten();

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    format!("{}-{:03}", prefix, next_seq)
}

/// パートナー単位で支払通知書を集約作成
pub async fn create_notices_by_partner(
    pool: &PgPool,
    view_rows: &[SettlementViewRow],
    target_month: NaiveDate,
) -> Result<Vec<CreatedNotice>, String> {
    use std::collections::{HashMap, HashSet};
    use crate::infrastructure::repositories::order_repo;

    // パートナー×プロジェクト単位でグループ化（承認済み＋パートナー契約ありの行のみ）
    let mut groups: HashMap<(String, String), Vec<&SettlementViewRow>> = HashMap::new();
    for vr in view_rows {
        if vr.row.timesheet_status.as_deref() != Some("APPROVED") {
            continue;
        }
        // 社員（パートナー契約なし）はスキップ
        if vr.row.partner_contract_id.is_none() {
            continue;
        }
        if let Some(ref pid) = vr.row.partner_id {
            groups.entry((pid.clone(), vr.row.project_id.clone()))
                .or_default()
                .push(vr);
        }
    }

    let threshold = get_approval_threshold(pool).await;
    let mut results = Vec::new();

    let mut tx = pool.begin().await.map_err(|e| format!("TX error: {}", e))?;

    for ((partner_id, project_id), rows) in &groups {
        if rows.is_empty() { continue; }

        // t_payment_notice.purchase_order_id は t_purchase_order への FK 必須。
        // ダミー値 "DASHBOARD" は FK 違反になるため、各行の契約・対象月から実在する発注を解決する。
        let mut eligible_rows: Vec<(&SettlementViewRow, String)> = Vec::new();
        let mut purchase_order_ids: HashSet<String> = HashSet::new();
        for vr in rows {
            let Some(pc_id) = vr.row.partner_contract_id else { continue; };
            match order_repo::find_purchase_order_id_for_contract_month(pool, pc_id, target_month).await {
                Ok(Some(order_id)) => {
                    purchase_order_ids.insert(order_id.clone());
                    eligible_rows.push((vr, order_id));
                }
                Ok(None) => {
                    tracing::warn!(
                        "支払通知スキップ: 発注注文書なし partner_contract_id={} project_id={} month={}",
                        pc_id, project_id, target_month
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "支払通知: 発注注文書検索エラー partner_contract_id={}: {:?}",
                        pc_id, e
                    );
                }
            }
        }
        if eligible_rows.is_empty() {
            tracing::warn!(
                "支払通知スキップ: 発行可能な発注がありません partner_id={} project_id={} month={}",
                partner_id, project_id, target_month
            );
            continue;
        }

        // ヘッダ FK 用の代表発注（集約明細は複数契約可。確認時は代表＋下記で全発注を NOTICE_CREATED へ）
        let header_purchase_order_id = eligible_rows[0].1.clone();

        let notice_id = generate_notice_id(pool, target_month).await;
        let subtotal: i32 = eligible_rows.iter().map(|(r, _)| r.payment_amount).sum();
        let tax_rate = crate::infrastructure::repositories::tax_rate_repo::find_effective_rate(pool, target_month)
            .await
            .unwrap_or(Decimal::from(10));
        let breakdown = crate::domain::services::tax_calculation::calculate_tax_breakdown(&[(tax_rate, subtotal)]);
        let tax_amount = crate::domain::services::tax_calculation::total_tax_amount(&breakdown);
        let total = subtotal + tax_amount;
        let needs_approval = total >= threshold;
        let approval_status = if needs_approval {
            ApprovalStatus::PendingApproval
        } else {
            ApprovalStatus::None
        };

        // 対象月末日＋パートナー契約の支払条件から支払期日を算出（請求書 due_date と同じロジック）
        let month_end = if target_month.month() == 12 {
            NaiveDate::from_ymd_opt(target_month.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(target_month.year(), target_month.month() + 1, 1)
        }
        .and_then(|d| d.pred_opt())
        .unwrap_or(target_month);
        let payment_terms = eligible_rows.iter()
            .map(|(r, _)| r.row.payment_condition.as_str())
            .find(|t| !t.trim().is_empty())
            .unwrap_or(DEFAULT_CLIENT_PAYMENT_TERMS);
        let payment_due_date = payment_due_date_from_terms(month_end, payment_terms);

        // 支払通知書ヘッダ作成
        if let Err(e) = settlement_repo::insert_payment_notice(
            &mut tx,
            &notice_id,
            &header_purchase_order_id,
            partner_id,
            target_month,
            payment_due_date,
            subtotal,
            tax_amount,
            total,
            approval_status.as_str(),
        ).await {
            tracing::error!("支払通知書作成エラー: {:?}", e);
            continue;
        }

        // 明細作成
        for (vr, _) in &eligible_rows {
            if let Err(e) = settlement_repo::insert_payment_notice_item(
                &mut tx,
                &notice_id,
                vr.row.partner_contract_id.unwrap_or(0),
                vr.row.total_hours.unwrap_or(Decimal::ZERO),
                vr.row.payment_base_rate.unwrap_or(0),
                vr.row.payment_effort.unwrap_or(Decimal::ONE),
                vr.row.payment_lower_limit.unwrap_or(Decimal::ZERO),
                vr.row.payment_upper_limit.unwrap_or(Decimal::ZERO),
                vr.row.payment_fixed_hours,
                vr.row.payment_deduction_rate.unwrap_or(0),
                vr.row.payment_overtime_rate.unwrap_or(0),
                vr.payment_amount - vr.row.payment_base_rate.unwrap_or(0),
                vr.payment_amount,
                tax_rate,
            ).await {
                tracing::error!("支払通知明細作成エラー: {:?}", e);
            }
        }

        // 紐づく発注をすべて NOTICE_CREATED へ（個別 /notices 作成と同じ）
        for order_id in &purchase_order_ids {
            if let Err(e) = settlement_repo::update_purchase_order_status_to_notice_created(&mut tx, order_id).await {
                tracing::error!("発注ステータス更新エラー order_id={}: {:?}", order_id, e);
            }
        }

        results.push(CreatedNotice {
            notice_id,
            partner_id: partner_id.clone(),
            partner_name: eligible_rows.first().map(|(r, _)| r.row.partner_name.clone().unwrap_or_default()).unwrap_or_default(),
            project_id: project_id.clone(),
            project_name: eligible_rows.first().map(|(r, _)| r.row.project_name.clone()).unwrap_or_default(),
            count: eligible_rows.len(),
            total,
            needs_approval,
        });
    }

    tx.commit().await.map_err(|e| format!("Commit error: {}", e))?;
    Ok(results)
}

/// 作成結果
#[derive(Debug, Clone, Serialize)]
pub struct CreatedNotice {
    pub notice_id: String,
    pub partner_id: String,
    pub partner_name: String,
    pub project_id: String,
    pub project_name: String,
    pub count: usize,
    pub total: i32,
    pub needs_approval: bool,
}

/// 選択行ごとに支払通知を発行できない理由を診断する。
pub async fn diagnose_notice_issue_blockers(
    pool: &PgPool,
    view_rows: &[SettlementViewRow],
    target_month: NaiveDate,
) -> Vec<ActionBlocker> {
    use crate::domain::models::partner_contract::PurchaseOrderStatus;
    use crate::infrastructure::repositories::order_repo;

    let month_label = target_month.format("%Y年%m月").to_string();
    let mut blockers = Vec::new();

    for vr in view_rows {
        let r = &vr.row;
        let engineer = r.engineer_name.clone();
        let partner = r.partner_name.clone().unwrap_or_else(|| "（パートナー未設定）".into());
        let project = r.project_name.clone();

        if r.notice_issued.unwrap_or(false) {
            blockers.push(ActionBlocker {
                subject: engineer.clone(),
                context: format!("{} / {}", partner, project),
                reason: format!("{month_label}分の支払通知は既に発行済みです"),
                suggestion: "再発行は不要です。内容の確認・送付は支払通知一覧から行ってください。".into(),
                link_path: "/notices".into(),
                link_label: "支払通知一覧を開く".into(),
                code: "notice_already_issued".into(),
            });
            continue;
        }

        let Some(pc_id) = r.partner_contract_id else {
            let suggestion = format!(
                "「発注契約」で {} さん・案件「{}」・{} をカバーする契約を登録してください。",
                engineer, project, month_label
            );
            blockers.push(ActionBlocker {
                subject: engineer.clone(),
                context: format!("{} / {}", partner, project),
                reason: "発注契約（パートナー契約）が紐づいていません（自社エンジニア扱い）".into(),
                suggestion,
                link_path: "/partner-contracts".into(),
                link_label: "発注契約一覧を開く".into(),
                code: "no_partner_contract".into(),
            });
            continue;
        };

        if r.timesheet_status.as_deref() != Some("APPROVED") {
            let (code, reason, suggestion, link_path, link_label) = if r.timesheet_id.is_none() {
                (
                    "timesheet_missing".to_string(),
                    format!("{month_label}分の稼働報告が未提出です"),
                    "稼働報告を登録し、承認してから支払通知を発行してください。".to_string(),
                    "/timesheets?upload=1".to_string(),
                    "稼働報告の登録画面を開く".to_string(),
                )
            } else {
                (
                    "timesheet_not_approved".to_string(),
                    format!("{month_label}分の稼働報告が未承認です"),
                    "稼働報告を承認してから支払通知を発行してください。".to_string(),
                    format!("/timesheets?edit={}", r.timesheet_id.unwrap_or(0)),
                    "稼働報告の承認画面を開く".to_string(),
                )
            };
            blockers.push(ActionBlocker {
                subject: engineer.clone(),
                context: format!("{} / {}", partner, project),
                reason,
                suggestion,
                link_path,
                link_label,
                code,
            });
            continue;
        }

        match order_repo::find_purchase_order_status_for_contract_month(pool, pc_id, target_month).await
        {
            Ok(None) => {
                blockers.push(ActionBlocker {
                    subject: engineer.clone(),
                    context: format!("{} / {}", partner, project),
                    reason: format!("{month_label}分の発注注文書がありません"),
                    suggestion: "「発注一覧」で対象月の発注注文書を作成し、パートナーの承諾（受諾済）まで進めてください。".into(),
                    link_path: "/orders".into(),
                    link_label: "発注一覧を開く".into(),
                    code: "purchase_order_missing".into(),
                });
            }
            Ok(Some((order_id, status))) => {
                let status_enum = PurchaseOrderStatus::from_str(&status);
                if matches!(
                    status_enum,
                    PurchaseOrderStatus::Accepted | PurchaseOrderStatus::ReportReceived
                ) {
                    // 発注は問題なし。他要因（DBエラー等）の可能性 — 汎用メッセージは出さない
                    continue;
                }
                let status_label = status_enum.display();
                let (suggestion, link_path, link_label) = match status_enum {
                    PurchaseOrderStatus::Draft => (
                        "発注注文書を送付し、パートナーの承諾を得てください（ステータスが「受諾済」または「報告書受領」になる必要があります）。".to_string(),
                        format!("/orders/{order_id}"),
                        "発注書詳細を開く".to_string(),
                    ),
                    PurchaseOrderStatus::Sent => (
                        "パートナーが発注書を承諾するまでお待ちください。承諾後に支払通知を発行できます。".to_string(),
                        format!("/orders/{order_id}"),
                        "発注書詳細を開く".to_string(),
                    ),
                    PurchaseOrderStatus::NoticeCreated
                    | PurchaseOrderStatus::NoticeConfirmed
                    | PurchaseOrderStatus::Paid => (
                        format!(
                            "発注書（{order_id}）は既に支払通知処理済み（現在: {status_label}）です。新規発行はできません。"
                        ),
                        "/notices".to_string(),
                        "支払通知一覧を開く".to_string(),
                    ),
                    _ => (
                        format!(
                            "発注書（{order_id}）のステータスが「{status_label}」のため発行できません。「受諾済」または「報告書受領」にしてください。"
                        ),
                        format!("/orders/{order_id}"),
                        "発注書詳細を開く".to_string(),
                    ),
                };
                blockers.push(ActionBlocker {
                    subject: engineer.clone(),
                    context: format!("{} / {}", partner, project),
                    reason: format!(
                        "発注注文書 {order_id} のステータスが「{status_label}」です（発行には「受諾済」または「報告書受領」が必要）"
                    ),
                    suggestion,
                    link_path,
                    link_label,
                    code: "purchase_order_wrong_status".into(),
                });
            }
            Err(e) => {
                tracing::error!("支払通知診断: 発注検索エラー partner_contract_id={pc_id}: {e:?}");
                blockers.push(ActionBlocker {
                    subject: engineer.clone(),
                    context: format!("{} / {}", partner, project),
                    reason: "発注注文書の確認中にエラーが発生しました".into(),
                    suggestion: "しばらく待ってから再試行するか、管理者に連絡してください。".into(),
                    link_path: "/orders".into(),
                    link_label: "発注一覧を開く".into(),
                    code: "internal_error".into(),
                });
            }
        }
    }

    blockers
}

/// 診断結果をユーザー向けメッセージ1本にまとめる（後方互換性のため残す）
pub fn format_notice_issue_blockers_message(blockers: &[ActionBlocker]) -> String {
    use crate::domain::models::actionable_error::format_action_blockers_message;
    format_action_blockers_message(blockers, "支払通知書を発行できませんでした。条件を確認してください。")
}

/// 選択行ごとに請求書を発行できない理由を診断する。
pub async fn diagnose_invoice_issue_blockers(
    pool: &PgPool,
    view_rows: &[SettlementViewRow],
    target_month: NaiveDate,
) -> Vec<ActionBlocker> {
    let month_label = target_month.format("%Y年%m月").to_string();
    let mut blockers = Vec::new();

    for vr in view_rows {
        let r = &vr.row;
        let engineer = r.engineer_name.clone();
        let client = r.client_name.clone();
        let project = r.project_name.clone();

        // 既に発行済み
        if r.invoice_issued.unwrap_or(false) {
            blockers.push(ActionBlocker {
                subject: engineer.clone(),
                context: format!("{} / {}", client, project),
                reason: format!("{month_label}分の請求書は既に発行済みです"),
                suggestion: "再発行は不要です。内容の確認・送付は請求書一覧から行ってください。".into(),
                link_path: "/invoices".into(),
                link_label: "請求書一覧を開く".into(),
                code: "invoice_already_issued".into(),
            });
            continue;
        }

        // EDI_OASIS クライアント（対象外）— 発行0件の理由としてカウント
        if r.client_edi_system_type == "EDI_OASIS" {
            blockers.push(ActionBlocker {
                subject: engineer.clone(),
                context: format!("{} / {}", client, project),
                reason: format!("{client}はEDI連携クライアント（先方システムで請求書を発行するため対象外）です"),
                suggestion: "請求書は発行していません。発注側のEDIシステムにて処理してください。".into(),
                link_path: "/invoices".into(),
                link_label: "請求書一覧を開く".into(),
                code: "edi_client_excluded".into(),
            });
            continue;
        }

        // 稼働報告未承認
        if r.timesheet_status.as_deref() != Some("APPROVED") {
            let (code, reason, suggestion, link_path, link_label) = if r.timesheet_id.is_none() {
                (
                    "timesheet_missing".to_string(),
                    format!("{month_label}分の稼働報告が未提出です"),
                    "稼働報告を登録し、承認してから請求書を発行してください。".to_string(),
                    "/timesheets?upload=1".to_string(),
                    "稼働報告の登録画面を開く".to_string(),
                )
            } else {
                (
                    "timesheet_not_approved".to_string(),
                    format!("{month_label}分の稼働報告が未承認です"),
                    "稼働報告を承認してから請求書を発行してください。".to_string(),
                    format!("/timesheets?edit={}", r.timesheet_id.unwrap_or(0)),
                    "稼働報告の承認画面を開く".to_string(),
                )
            };
            blockers.push(ActionBlocker {
                subject: engineer.clone(),
                context: format!("{} / {}", client, project),
                reason,
                suggestion,
                link_path,
                link_label,
                code,
            });
            continue;
        }

        // 受注書未設定（簡易チェック）
        // 詳細な受注書存在確認はここでは行わない（create_invoices_by_client内で既に実施）
    }

    blockers
}

/// 請求書番号を自動採番する（INV-YYYYMM-001）
pub async fn generate_invoice_no(pool: &PgPool, target_month: NaiveDate) -> String {
    let prefix = format!("INV-{}", target_month.format("%Y%m"));
    let max_seq: Option<String> = settlement_repo::max_invoice_no_with_prefix(pool, &prefix)
        .await
        .ok()
        .flatten();

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    format!("{}-{:03}", prefix, next_seq)
}

/// クライアント契約の支払条件テキストが空のときに使うデフォルト
/// （m_client_contract.payment_terms のカラムDEFAULTと同じ）
const DEFAULT_CLIENT_PAYMENT_TERMS: &str = "毎月末日締め翌月末日払い";

/// 支払条件テキストから支払期日を算出する。
/// 「翌々月15日払い」→ work_end + 2ヶ月の15日 / 「翌月末日払い」→ work_end + 1ヶ月の末日。
/// 空・未認識はデフォルト（毎月末日締め翌月末日払い）として扱う。
pub fn payment_due_date_from_terms(work_end: NaiveDate, payment_terms: &str) -> NaiveDate {
    let cond = if payment_terms.trim().is_empty() {
        DEFAULT_CLIENT_PAYMENT_TERMS
    } else {
        payment_terms
    };

    let pay_day = |c: &str| -> u32 {
        if c.contains("15日") || c.contains("１５日") {
            15
        } else if c.contains("末日払") || c.contains("末払") {
            0 // 末日
        } else {
            // デフォルト相当（翌月末日払い）
            0
        }
    };
    let add_months = |year: i32, month: u32, add: u32| -> (i32, u32) {
        let total = month + add;
        if total > 12 {
            (year + (total as i32 - 1) / 12, ((total - 1) % 12) + 1)
        } else {
            (year, total)
        }
    };
    let last_day = |year: i32, month: u32| -> u32 {
        let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        NaiveDate::from_ymd_opt(ny, nm, 1)
            .and_then(|d| d.pred_opt())
            .map(|p| p.day())
            .unwrap_or(28)
    };

    let offset = if cond.contains("翌々月") {
        2
    } else {
        // 「翌月…」および未認識 → 翌月
        1
    };
    let (y, m) = add_months(work_end.year(), work_end.month(), offset);
    let raw_day = pay_day(cond);
    let day = if raw_day == 0 { last_day(y, m) } else { raw_day };
    NaiveDate::from_ymd_opt(y, m, day).unwrap_or(work_end)
}

/// クライアント単位で請求書を集約作成
///
/// 支払通知書(`create_notices_by_partner`)と対になる、クライアント向け請求書の集約発行。
/// パートナー有無に関わらず、承認済み(APPROVED)の行は全てクライアントへの請求対象になる
/// （支払通知書はパートナー契約がある行のみが対象なのとは非対称）。
pub async fn create_invoices_by_client(
    pool: &PgPool,
    view_rows: &[SettlementViewRow],
    target_month: NaiveDate,
) -> Result<Vec<CreatedInvoice>, String> {
    use std::collections::HashMap;

    // クライアント単位でグループ化（承認済み、かつ先方が外部EDIシステム(OASIS等)を
    // 保有していない行のみ。EDI保有クライアントの請求書は先方のEDI経由で受け取るため、
    // Sophiaから新規発行すると重複してしまう）
    //
    // 判定は edi_system_type の非空判定ではなく "EDI_OASIS" との完全一致で行う。
    // 非空判定だと「メールで受け取っている」等の備考的な値を入れただけの非EDI連携
    // クライアント（例: クロスシステム、edi_system_type='EMAIL'）まで誤って除外され、
    // 本来Sophia側で発行すべき請求書が発行されなくなる不具合が実際に発生した。
    let mut groups: HashMap<(i64, String), Vec<&SettlementViewRow>> = HashMap::new();
    for vr in view_rows {
        if vr.row.timesheet_status.as_deref() != Some("APPROVED") {
            continue;
        }
        if vr.row.client_edi_system_type == "EDI_OASIS" {
            continue;
        }
        groups.entry((vr.row.client_id, vr.row.project_id.clone())).or_default().push(vr);
    }

    let month_end = if target_month.month() == 12 {
        NaiveDate::from_ymd_opt(target_month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(target_month.year(), target_month.month() + 1, 1)
    }
    .and_then(|d| d.pred_opt())
    .unwrap_or(target_month);

    let mut results = Vec::new();
    let mut tx = pool.begin().await.map_err(|e| format!("TX error: {}", e))?;

    for ((client_id, project_id), rows) in &groups {
        if rows.is_empty() { continue; }

        let invoice_no = generate_invoice_no(pool, target_month).await;
        let subtotal: i32 = rows.iter().map(|r| r.billing_amount).sum();
        let tax_rate = crate::infrastructure::repositories::tax_rate_repo::find_effective_rate(pool, target_month)
            .await
            .unwrap_or(Decimal::from(10));
        let breakdown = crate::domain::services::tax_calculation::calculate_tax_breakdown(&[(tax_rate, subtotal)]);
        let tax_amount = crate::domain::services::tax_calculation::total_tax_amount(&breakdown);
        let total = subtotal + tax_amount;
        let project_name = rows.first().map(|r| r.row.project_name.clone()).unwrap_or_default();
        let subject = format!("{} {}年{:02}月分 請求書", project_name, target_month.year(), target_month.month());
        let client_name = rows.first().map(|r| r.row.client_name.clone()).unwrap_or_default();
        // グループ内で非空の支払条件を優先。全て空ならカラムDEFAULT相当を適用。
        let payment_terms = rows.iter()
            .map(|r| r.row.billing_payment_terms.as_str())
            .find(|t| !t.trim().is_empty())
            .unwrap_or(DEFAULT_CLIENT_PAYMENT_TERMS);
        let due_date = payment_due_date_from_terms(month_end, payment_terms);

        // 請求書ヘッダ作成
        let invoice_id: i64 = match settlement_repo::insert_invoice_header(
            &mut tx,
            &invoice_no,
            *client_id,
            target_month,
            month_end,
            &subject,
            subtotal,
            tax_amount,
            total,
            due_date,
        ).await {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("請求書作成エラー: {:?}", e);
                continue;
            }
        };

        // 明細作成 + 対応する受注のステータスをINVOICEDへ進める
        let mut header_received_order_id: Option<i64> = None;
        for vr in rows {
            let _ = settlement_repo::insert_invoice_item(
                &mut tx,
                invoice_id,
                vr.row.engineer_id,
                &vr.row.engineer_name,
                vr.row.total_hours.unwrap_or(Decimal::ZERO).to_i32().unwrap_or(0),
                vr.row.billing_base_rate,
                vr.billing_amount,
                &vr.row.billing_settlement_type,
                vr.row.billing_lower_limit,
                vr.row.billing_upper_limit,
                vr.row.billing_deduction_rate,
                vr.row.billing_overtime_rate,
                tax_rate,
            ).await;

            let ro_id = settlement_repo::find_received_order_for_invoice(
                &mut tx,
                *client_id,
                target_month,
                vr.row.engineer_id,
                vr.row.client_contract_id,
            )
            .await
            .ok()
            .flatten();

            if let Some(ro_id) = ro_id {
                if header_received_order_id.is_none() {
                    header_received_order_id = Some(ro_id);
                }
                let _ = settlement_repo::link_received_order_invoice(&mut tx, ro_id, invoice_id).await;
            }
        }

        if let Some(ro_id) = header_received_order_id {
            let _ = settlement_repo::set_invoice_received_order(&mut tx, invoice_id, ro_id).await;
        }

        results.push(CreatedInvoice {
            id: invoice_id,
            invoice_no,
            client_id: *client_id,
            client_name,
            project_id: project_id.clone(),
            project_name,
            count: rows.len(),
            total,
        });
    }

    tx.commit().await.map_err(|e| format!("Commit error: {}", e))?;
    Ok(results)
}

/// 作成結果（請求書）
#[derive(Debug, Clone, Serialize)]
pub struct CreatedInvoice {
    pub id: i64,
    pub invoice_no: String,
    pub client_id: i64,
    pub client_name: String,
    pub project_id: String,
    pub project_name: String,
    pub count: usize,
    pub total: i32,
}

#[cfg(test)]
mod payment_due_date_tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn next_month_end_from_default_terms() {
        assert_eq!(
            payment_due_date_from_terms(ymd(2026, 7, 31), "毎月末日締め翌月末日払い"),
            ymd(2026, 8, 31)
        );
    }

    #[test]
    fn empty_terms_uses_default_next_month_end() {
        assert_eq!(
            payment_due_date_from_terms(ymd(2026, 7, 31), ""),
            ymd(2026, 8, 31)
        );
    }

    #[test]
    fn after_next_month_15th() {
        assert_eq!(
            payment_due_date_from_terms(ymd(2026, 5, 31), "翌々月15日払い"),
            ymd(2026, 7, 15)
        );
    }
}
