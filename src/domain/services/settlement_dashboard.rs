/// domain/services/settlement_dashboard.rs — 月次確定ダッシュボード ドメインサービス
///
/// ダッシュボード画面のビジネスロジックを集約。
/// - パートナー単位の支払通知書集約発行
/// - 上司承認判定
/// - プレビュー計算

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::Serialize;
use sqlx::{PgPool, FromRow};

use crate::domain::models::partner_contract::ApprovalStatus;
use crate::domain::models::actionable_error::ActionBlocker;
use crate::infrastructure::repositories::settlement_repo;
use crate::infrastructure::db_tx::commit_checked;

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
    /// クライアントの EDI 方式（取引先マスタ「EDI方式」の値）。表示用。請求書の発行対象かの判定には使わない
    pub client_edi_system_type: String,
    /// この取引先の請求書は、先方のシステムで作る（作成代行）か（取引先マスタ `invoice_issued_by_client`）。
    /// TRUE なら、請求書は先方の EDI 経由で受け取る（Sophia から新規発行しない）
    pub client_invoice_by_client: bool,
    // 稼働報告
    pub timesheet_id: Option<i64>,
    pub total_hours: Option<Decimal>,
    pub timesheet_status: Option<String>,
    // 請求（売上）側
    pub client_contract_id: i64,
    /// 対象月の注文書（受注）。請求の条件・金額はここから取る。NULL = 注文書なし（請求書は作れない）
    pub received_order_id: Option<i64>,
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
        // 注文書が無い月は、請求金額を 0 とし、請求の対象にしない
        let billing_amount = if ts_approved && row.received_order_id.is_some() {
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

/// 支払通知書番号を自動採番する（PN-YYYYMM-001）。
/// 発行と同じトランザクションの中で、月ごとの排他ロックを取ってから最大値を読む。
/// (別の接続で読むと、同じ呼び出しで先に作った番号が見えず、同じ番号になってしまう)
pub async fn generate_notice_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_month: NaiveDate,
) -> Result<String, String> {
    let prefix = format!("PN-{}", target_month.format("%Y%m"));
    settlement_repo::lock_numbering(tx, &format!("notice_id:{}", target_month.format("%Y%m")))
        .await
        .map_err(|e| format!("採番のロックに失敗しました: {e}"))?;
    let max_seq: Option<String> = settlement_repo::max_notice_id_with_prefix(tx, &prefix)
        .await
        .map_err(|e| format!("支払通知書番号の採番に失敗しました: {e}"))?;

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    Ok(format!("{}-{:03}", prefix, next_seq))
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
                    return Err(format!("発注注文書の検索に失敗しました: {}", e));
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

        let notice_id = generate_notice_id(&mut tx, target_month).await?;
        let subtotal: i32 = eligible_rows.iter().map(|(r, _)| r.payment_amount).sum();
        let tax_rate = crate::infrastructure::repositories::tax_rate_repo::find_effective_rate(pool, target_month)
            .await
            .unwrap_or(Decimal::from(10));
        let breakdown = crate::domain::services::tax_calculation::calculate_tax_breakdown(&[(tax_rate, subtotal)]);
        let tax_amount = crate::domain::services::tax_calculation::total_tax_amount(&breakdown);
        let total = subtotal + tax_amount;
        // 支払通知は金額に関わらず常に承認が必要（請求書と同じ運用）
        let needs_approval = true;
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
        settlement_repo::insert_payment_notice(
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
        ).await
        .map_err(|e| {
            tracing::error!("支払通知書作成エラー: {:?}", e);
            format!("支払通知書ヘッダの登録に失敗しました: {}", e)
        })?;

        // 明細作成
        for (vr, _) in &eligible_rows {
            settlement_repo::insert_payment_notice_item(
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
            ).await
            .map_err(|e| {
                tracing::error!("支払通知明細作成エラー: {:?}", e);
                format!("支払通知書明細の登録に失敗しました: {}", e)
            })?;
        }

        // 紐づく発注をすべて NOTICE_CREATED へ（個別 /notices 作成と同じ）
        for order_id in &purchase_order_ids {
            settlement_repo::update_purchase_order_status_to_notice_created(&mut tx, order_id)
                .await
                .map_err(|e| {
                    tracing::error!("発注ステータス更新エラー order_id={}: {:?}", order_id, e);
                    format!("発注ステータス更新に失敗しました: {}", e)
                })?;
        }

        results.push((notice_id, partner_id.clone(), eligible_rows.first().map(|(r, _)| r.row.partner_name.clone().unwrap_or_default()).unwrap_or_default(), project_id.clone(), eligible_rows.first().map(|(r, _)| r.row.project_name.clone()).unwrap_or_default(), eligible_rows.len(), total, needs_approval));
    }

    commit_checked(tx).await.map_err(|e| format!("トランザクション確定エラー: {}", e))?;

    // 支払通知の保存を確認（DBから読み直す）
    let notice_ids: Vec<String> = results.iter().map(|(id, _, _, _, _, _, _, _)| id.clone()).collect();
    if notice_ids.is_empty() {
        return Ok(Vec::new());
    }

    let notice_rows = settlement_repo::find_payment_notices_by_ids(pool, &notice_ids)
        .await
        .map_err(|e| format!("支払通知の読み直しに失敗しました: {}", e))?;

    if notice_rows.len() != results.len() {
        tracing::error!(
            "支払通知の保存を確認できませんでした: 作成数={}, 読み直し数={}",
            results.len(),
            notice_rows.len()
        );
        return Err(format!(
            "支払通知の保存を確認できませんでした: 作成数={}, 読み直し数={}",
            results.len(),
            notice_rows.len()
        ));
    }

    // 読み直したデータでCreatedNoticeを作成する。
    // 並び順は保証されないので、通知書IDで突き合わせる
    let db_by_id: std::collections::HashMap<String, i32> = notice_rows.into_iter().collect();
    let mut final_results = Vec::with_capacity(results.len());
    for (created_id, partner_id, partner_name, project_id, project_name, count, _, needs_approval) in results {
        let Some(db_total) = db_by_id.get(&created_id).copied() else {
            tracing::error!("支払通知の保存を確認できませんでした: notice_id={} がDBに見つかりません", created_id);
            return Err(format!("支払通知の保存を確認できませんでした: notice_id={created_id}"));
        };
        final_results.push(CreatedNotice {
            notice_id: created_id,
            partner_id,
            partner_name,
            project_id,
            project_name,
            count,
            total: db_total,
            needs_approval,
        });
    }

    Ok(final_results)
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

        // 請求書を先方が作る取引先（対象外）— 発行0件の理由としてカウント
        if r.client_invoice_by_client {
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

/// 請求書番号を自動採番する（INV-YYYYMM-001）。
/// 発行と同じトランザクションの中で、月ごとの排他ロックを取ってから最大値を読む(理由は generate_notice_id と同じ)。
pub async fn generate_invoice_no(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_month: NaiveDate,
) -> Result<String, String> {
    let prefix = format!("INV-{}", target_month.format("%Y%m"));
    settlement_repo::lock_numbering(tx, &format!("invoice_no:{}", target_month.format("%Y%m")))
        .await
        .map_err(|e| format!("採番のロックに失敗しました: {e}"))?;
    let max_seq: Option<String> = settlement_repo::max_invoice_no_with_prefix(tx, &prefix)
        .await
        .map_err(|e| format!("請求書番号の採番に失敗しました: {e}"))?;

    let next_seq = match max_seq {
        Some(max) => {
            let parts: Vec<&str> = max.rsplitn(2, '-').collect();
            let seq: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            seq + 1
        }
        None => 1,
    };

    Ok(format!("{}-{:03}", prefix, next_seq))
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

/// 請求書の単位(取引先ごとの設定 m_client.billing_unit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingUnit {
    /// 案件ごと(既定。同じ取引先でも案件ごとに1通)
    Project,
    /// 取引先まとめ(その月の全案件・全要員を1通)
    Client,
}

impl BillingUnit {
    /// DBの値から解釈する。'CLIENT' 以外(空・未知を含む)は、既定の案件ごと
    pub fn from_db(s: &str) -> Self {
        if s == "CLIENT" { Self::Client } else { Self::Project }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "PROJECT",
            Self::Client => "CLIENT",
        }
    }
}

/// 請求書の集約キー。案件ごと=(取引先, 案件)、取引先まとめ=(取引先, なし)
pub fn invoice_group_key(unit: BillingUnit, client_id: i64, project_id: &str) -> (i64, Option<String>) {
    match unit {
        BillingUnit::Project => (client_id, Some(project_id.to_string())),
        BillingUnit::Client => (client_id, None),
    }
}

/// 請求書の件名。案件ごと=「{案件名} YYYY年MM月分 請求書」(従来どおり)、取引先まとめ=「{取引先名} YYYY年MM月分 請求書」
pub fn invoice_subject(unit: BillingUnit, project_name: &str, client_name: &str, month: NaiveDate) -> String {
    let head = match unit {
        BillingUnit::Project => project_name,
        BillingUnit::Client => client_name,
    };
    format!("{} {}年{:02}月分 請求書", head, month.year(), month.month())
}

/// クライアント単位で請求書を集約作成
///
/// 請求書の単位は取引先ごとの設定(`BillingUnit`)。既定は案件ごと。
///
/// 支払通知書(`create_notices_by_partner`)と対になる、クライアント向け請求書の集約発行。
/// パートナー有無に関わらず、承認済み(APPROVED)の行は全てクライアントへの請求対象になる
/// （支払通知書はパートナー契約がある行のみが対象なのとは非対称）。
pub async fn create_invoices_by_client(
    pool: &PgPool,
    view_rows: &[SettlementViewRow],
    target_month: NaiveDate,
) -> Result<Vec<CreatedInvoice>, String> {
    let mut tx = pool.begin().await.map_err(|e| format!("TX error: {}", e))?;
    let issued = issue_invoices_in_tx(pool, &mut tx, view_rows, target_month).await?;
    commit_checked(tx).await.map_err(|e| format!("トランザクション確定エラー: {}", e))?;
    verify_issued_invoices(pool, issued).await
}

/// 請求書の税額と税込合計。プレビューと発行で同じ計算を使う(金額がずれないように)
pub fn invoice_totals(tax_rate: Decimal, subtotal: i32) -> (i32, i32) {
    let breakdown = crate::domain::services::tax_calculation::calculate_tax_breakdown(&[(tax_rate, subtotal)]);
    let tax_amount = crate::domain::services::tax_calculation::total_tax_amount(&breakdown);
    (tax_amount, subtotal + tax_amount)
}

/// トランザクション内で発行した請求書(確定前)
#[derive(Debug, Clone)]
pub struct IssuedInvoice {
    pub id: i64,
    pub invoice_no: String,
    pub client_id: i64,
    pub total: i32,
    pub client_name: String,
    pub project_id: String,
    pub project_name: String,
    pub count: usize,
}

/// 請求書を、渡されたトランザクションの中で発行する(確定は呼び出し側)。
/// 取引先の請求単位に従って集約する。`create_invoices_by_client`(旧画面の一括発行)と、
/// 確定API(`billing_confirm`)の両方がこれを使う。
pub async fn issue_invoices_in_tx(
    pool: &PgPool,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    view_rows: &[SettlementViewRow],
    target_month: NaiveDate,
) -> Result<Vec<IssuedInvoice>, String> {
    use std::collections::HashMap;

    // クライアント単位でグループ化（承認済み、かつ請求書を先方が作る取引先でない行のみ。
    // 先方が作る取引先の請求書は先方の EDI 経由で受け取るため、Sophia から新規発行すると重複してしまう）
    //
    // 判定は取引先マスタの `invoice_issued_by_client` だけで行う。以前は EDI 方式の値で判定しており、
    // 非空判定にした時期には「メールで受け取っている」（EMAIL）だけの取引先まで誤って除外され、
    // 本来 Sophia 側で発行すべき請求書が発行されなくなる不具合が実際に発生した。
    let mut candidates: Vec<&SettlementViewRow> = Vec::new();
    for vr in view_rows {
        if vr.row.timesheet_status.as_deref() != Some("APPROVED") {
            continue;
        }
        // 注文書が無い行は請求書を作らない（請求は注文書が正本）
        if vr.row.received_order_id.is_none() {
            continue;
        }
        if vr.row.client_invoice_by_client {
            continue;
        }
        candidates.push(vr);
    }

    // 取引先ごとの請求単位(既定は案件ごと=従来どおり)
    let mut client_ids: Vec<i64> = candidates.iter().map(|vr| vr.row.client_id).collect();
    client_ids.sort_unstable();
    client_ids.dedup();
    let units: HashMap<i64, BillingUnit> = settlement_repo::client_billing_units(pool, &client_ids)
        .await
        .map_err(|e| format!("請求単位の取得に失敗しました: {e}"))?
        .into_iter()
        .map(|(id, s)| (id, BillingUnit::from_db(&s)))
        .collect();
    let unit_of = |client_id: i64| units.get(&client_id).copied().unwrap_or(BillingUnit::Project);

    let mut groups: HashMap<(i64, Option<String>), Vec<&SettlementViewRow>> = HashMap::new();
    for vr in candidates {
        let key = invoice_group_key(unit_of(vr.row.client_id), vr.row.client_id, &vr.row.project_id);
        groups.entry(key).or_default().push(vr);
    }

    let month_end = if target_month.month() == 12 {
        NaiveDate::from_ymd_opt(target_month.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(target_month.year(), target_month.month() + 1, 1)
    }
    .and_then(|d| d.pred_opt())
    .unwrap_or(target_month);

    let mut results: Vec<IssuedInvoice> = Vec::new();

    for ((client_id, group_project_id), rows) in &groups {
        if rows.is_empty() { continue; }
        let unit = unit_of(*client_id);

        let invoice_no = generate_invoice_no(tx, target_month).await?;
        let subtotal: i32 = rows.iter().map(|r| r.billing_amount).sum();
        let tax_rate = crate::infrastructure::repositories::tax_rate_repo::find_effective_rate(pool, target_month)
            .await
            .unwrap_or(Decimal::from(10));
        let (tax_amount, total) = invoice_totals(tax_rate, subtotal);
        let project_name = rows.first().map(|r| r.row.project_name.clone()).unwrap_or_default();
        let client_name = rows.first().map(|r| r.row.client_name.clone()).unwrap_or_default();
        let subject = invoice_subject(unit, &project_name, &client_name, target_month);
        // グループ内で非空の支払条件を優先。全て空ならカラムDEFAULT相当を適用。
        let payment_terms = rows.iter()
            .map(|r| r.row.billing_payment_terms.as_str())
            .find(|t| !t.trim().is_empty())
            .unwrap_or(DEFAULT_CLIENT_PAYMENT_TERMS);
        let due_date = payment_due_date_from_terms(month_end, payment_terms);

        // 請求書ヘッダ作成
        let invoice_id: i64 = match settlement_repo::insert_invoice_header(
            tx,
            &invoice_no,
            *client_id,
            target_month,
            month_end,
            &subject,
            subtotal,
            tax_amount,
            total,
            due_date,
            group_project_id.as_deref(),
            unit.as_str(),
        ).await {
            Ok(id) => id,
            Err(e) => {
                // 1件でも失敗したら全体を取り消して、失敗として返す（途中の失敗を捨てて先へ進むと、
                // PostgreSQL では以降の文がすべて無効になり、確定しても取り消されて、成功と表示されたのに
                // 何も保存されない状態になる）
                tracing::error!("請求書作成エラー: {:?}", e);
                return Err(format!("請求書ヘッダの登録に失敗しました: {e}"));
            }
        };

        // 明細作成 + 対応する受注のステータスをINVOICEDへ進める
        // どの文が失敗しても、結果を捨てずに失敗として返す（全体が取り消され、画面にエラーが出る）
        let mut header_received_order_id: Option<i64> = None;
        for vr in rows {
            settlement_repo::insert_invoice_item(
                tx,
                invoice_id,
                vr.row.engineer_id,
                &vr.row.engineer_name,
                // 稼働時間（例: 140.5）。DBの列は小数に対応しているので、切り捨てずに保存する
                vr.row.total_hours.unwrap_or(Decimal::ZERO),
                vr.row.billing_base_rate,
                vr.billing_amount,
                &vr.row.billing_settlement_type,
                vr.row.billing_lower_limit,
                vr.row.billing_upper_limit,
                vr.row.billing_deduction_rate,
                vr.row.billing_overtime_rate,
                tax_rate,
            )
            .await
            .map_err(|e| format!("請求書明細の登録に失敗しました: {e}"))?;

            let ro_id = settlement_repo::find_received_order_for_invoice(
                tx,
                *client_id,
                target_month,
                vr.row.engineer_id,
                vr.row.client_contract_id,
            )
            .await
            .map_err(|e| format!("受注の検索に失敗しました: {e}"))?;

            if let Some(ro_id) = ro_id {
                if header_received_order_id.is_none() {
                    header_received_order_id = Some(ro_id);
                }
                settlement_repo::link_received_order_invoice(tx, ro_id)
                    .await
                    .map_err(|e| format!("受注の状態更新に失敗しました: {e}"))?;
            }
        }

        if let Some(ro_id) = header_received_order_id {
            settlement_repo::set_invoice_received_order(tx, invoice_id, ro_id)
                .await
                .map_err(|e| format!("請求書と受注の紐づけに失敗しました: {e}"))?;
        }

        // 取引先まとめでは案件が複数にまたがるので、案件欄は「全案件」とする
        let (result_project_id, result_project_name) = match group_project_id {
            Some(pid) => (pid.clone(), project_name),
            None => (String::new(), "全案件".to_string()),
        };
        results.push(IssuedInvoice { id: invoice_id, invoice_no, client_id: *client_id, total, client_name, project_id: result_project_id, project_name: result_project_name, count: rows.len() });
    }

    Ok(results)
}

/// 確定後に、請求書をDBから読み直して件数・IDを突き合わせる(保存を確認する)
pub async fn verify_issued_invoices(
    pool: &PgPool,
    issued: Vec<IssuedInvoice>,
) -> Result<Vec<CreatedInvoice>, String> {
    let invoice_ids: Vec<i64> = issued.iter().map(|r| r.id).collect();
    if invoice_ids.is_empty() {
        return Ok(Vec::new());
    }

    let invoice_rows = settlement_repo::find_invoices_by_ids(pool, &invoice_ids)
        .await
        .map_err(|e| format!("請求書の読み直しに失敗しました: {}", e))?;

    if invoice_rows.len() != issued.len() {
        tracing::error!(
            "請求書の保存を確認できませんでした: 作成数={}, 読み直し数={}",
            issued.len(),
            invoice_rows.len()
        );
        return Err(format!(
            "請求書の保存を確認できませんでした: 作成数={}, 読み直し数={}",
            issued.len(),
            invoice_rows.len()
        ));
    }

    // 並び順は保証されないので、IDで突き合わせる（位置で突き合わせると、別の請求書の表示になり得る）
    let db_by_id: std::collections::HashMap<i64, (String, i64, i32)> = invoice_rows
        .into_iter()
        .map(|(id, no, client_id, total)| (id, (no, client_id, total)))
        .collect();
    let mut final_results = Vec::with_capacity(issued.len());
    for r in issued {
        let Some((db_invoice_no, db_client_id, db_total)) = db_by_id.get(&r.id).cloned() else {
            tracing::error!("請求書の保存を確認できませんでした: id={} がDBに見つかりません", r.id);
            return Err(format!("請求書の保存を確認できませんでした: id={}", r.id));
        };
        final_results.push(CreatedInvoice {
            id: r.id,
            invoice_no: db_invoice_no,
            client_id: db_client_id,
            client_name: r.client_name,
            project_id: r.project_id,
            project_name: r.project_name,
            count: r.count,
            total: db_total,
        });
    }
    Ok(final_results)
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

    fn sample_row(received_order_id: Option<i64>, base_rate: i32) -> SettlementRow {
        SettlementRow {
            engineer_id: 1,
            engineer_name: "テスト".to_string(),
            partner_id: None,
            partner_name: None,
            client_name: "クライアント".to_string(),
            client_id: 1,
            client_edi_system_type: String::new(),
            client_invoice_by_client: false,
            timesheet_id: Some(1),
            total_hours: Some(Decimal::from(160)),
            timesheet_status: Some("APPROVED".to_string()),
            client_contract_id: 1,
            received_order_id,
            billing_base_rate: base_rate,
            billing_settlement_type: "RANGE".to_string(),
            billing_lower_limit: Decimal::from(140),
            billing_upper_limit: Decimal::from(180),
            billing_fixed_hours: None,
            billing_deduction_rate: 0,
            billing_overtime_rate: 0,
            billing_effort: Decimal::ONE,
            billing_payment_terms: String::new(),
            partner_contract_id: None,
            payment_base_rate: None,
            payment_settlement_type: None,
            payment_lower_limit: None,
            payment_upper_limit: None,
            payment_fixed_hours: None,
            payment_deduction_rate: None,
            payment_overtime_rate: None,
            payment_effort: None,
            payment_condition: String::new(),
            invoice_issued: Some(false),
            notice_issued: None,
            purchase_order_id: None,
            purchase_order_status: None,
            project_id: "PRJ1".to_string(),
            project_name: "案件".to_string(),
            engineer_employee_code: String::new(),
        }
    }

    #[test]
    fn billing_uses_the_price_on_the_received_order() {
        // 注文書の単価（72万）で請求金額が決まる。上下限の範囲内（160h）なので、単価がそのまま請求額
        let (rows, _) = calculate_preview(vec![sample_row(Some(10), 720_000)]);
        assert_eq!(rows[0].billing_amount, 720_000);
        let (rows, _) = calculate_preview(vec![sample_row(Some(10), 700_000)]);
        assert_eq!(rows[0].billing_amount, 700_000);
    }

    #[test]
    fn billing_is_zero_without_a_received_order() {
        // 注文書が無い月は、稼働報告が承認済みでも、請求金額は 0（請求書は作れない）
        let (rows, _) = calculate_preview(vec![sample_row(None, 700_000)]);
        assert_eq!(rows[0].billing_amount, 0);
    }
}

#[cfg(test)]
mod billing_unit_tests {
    use super::*;

    fn month() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
    }

    #[test]
    fn billing_unit_parses_db_values_with_project_as_default() {
        assert_eq!(BillingUnit::from_db("CLIENT"), BillingUnit::Client);
        assert_eq!(BillingUnit::from_db("PROJECT"), BillingUnit::Project);
        // 空・未知は既定(案件ごと)。現行の挙動を変えない
        assert_eq!(BillingUnit::from_db(""), BillingUnit::Project);
        assert_eq!(BillingUnit::from_db("client"), BillingUnit::Project);
        assert_eq!(BillingUnit::Client.as_str(), "CLIENT");
        assert_eq!(BillingUnit::Project.as_str(), "PROJECT");
    }

    #[test]
    fn project_unit_splits_by_project_and_client_unit_merges_all_projects() {
        let a = invoice_group_key(BillingUnit::Project, 7, "PRJ1");
        let b = invoice_group_key(BillingUnit::Project, 7, "PRJ2");
        assert_ne!(a, b, "案件ごと: 案件が違えば別の請求書");
        assert_eq!(a, (7, Some("PRJ1".to_string())));

        let c = invoice_group_key(BillingUnit::Client, 7, "PRJ1");
        let d = invoice_group_key(BillingUnit::Client, 7, "PRJ2");
        assert_eq!(c, d, "取引先まとめ: 案件が違っても1通");
        assert_eq!(c, (7, None));
        assert_ne!(c, invoice_group_key(BillingUnit::Client, 8, "PRJ1"), "取引先が違えば別");
    }

    #[test]
    fn subject_uses_project_name_by_default_and_client_name_when_merged() {
        assert_eq!(
            invoice_subject(BillingUnit::Project, "A案件", "株式会社X", month()),
            "A案件 2026年10月分 請求書",
            "従来と同じ件名"
        );
        assert_eq!(
            invoice_subject(BillingUnit::Client, "A案件", "株式会社X", month()),
            "株式会社X 2026年10月分 請求書"
        );
    }
}
