/// domain/services/project_dashboard.rs — プロジェクト別ダッシュボード集計
///
/// ホームダッシュボードの「プロジェクト別」セクション用データを組み立てる。
/// 既存の settlement_dashboard::list_settlement_rows / calculate_preview を土台にする。

use std::collections::HashMap;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use sqlx::PgPool;

use crate::domain::services::settlement_dashboard::{
    self, SettlementFilter, SettlementViewRow,
};
use crate::infrastructure::repositories::payroll_repo;

/// プロジェクト1件分の集計結果
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub project_id: String,
    pub project_name: String,
    pub client_name: String,
    pub staff_internal_count: usize,
    pub staff_partner_count: usize,
    pub timesheet_approved_count: usize,
    pub timesheet_total_count: usize,
    pub pending_engineer_names: Vec<String>,
    pub billing_total: i32,
    pub partner_cost_total: i32,
    pub internal_cost_total: i32,
    pub profit: i32,
    /// 稼働報告が全員 PENDING(未提出)でない場合に true(確定)。1名でもPENDINGなら false(予定)
    pub is_finalized: bool,
    /// 詳細パネル展開用。この配下の要員1人1行
    pub engineers: Vec<SettlementViewRow>,
}

pub async fn list_project_summaries(pool: &PgPool, target_month: NaiveDate) -> anyhow::Result<Vec<ProjectSummary>> {
    let rows = settlement_dashboard::list_settlement_rows(
        pool, target_month, &SettlementFilter::default(),
    ).await?;
    let (view_rows, _summary) = settlement_dashboard::calculate_preview(rows);

    // ── 自社プロパーの「その月の全プロジェクト合計effort」を先に集計 ──
    let mut engineer_effort_totals: HashMap<i64, Decimal> = HashMap::new();
    for vr in &view_rows {
        if vr.row.partner_contract_id.is_none() {
            *engineer_effort_totals.entry(vr.row.engineer_id).or_insert(Decimal::ZERO)
                += vr.row.billing_effort;
        }
    }

    let internal_codes: Vec<String> = view_rows.iter()
        .filter(|vr| vr.row.partner_contract_id.is_none())
        .map(|vr| vr.row.engineer_employee_code.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let gross_pay_by_code = payroll_repo::get_gross_pay_by_employee_codes(
        pool, &internal_codes, target_month,
    ).await;

    // ── project_id でグルーピング ──
    let mut groups: HashMap<String, Vec<SettlementViewRow>> = HashMap::new();
    for vr in view_rows {
        groups.entry(vr.row.project_id.clone()).or_default().push(vr);
    }

    let mut summaries: Vec<ProjectSummary> = groups.into_iter().map(|(project_id, rows)| {
        let project_name = rows.first().map(|r| r.row.project_name.clone()).unwrap_or_default();
        let client_name = rows.first().map(|r| r.row.client_name.clone()).unwrap_or_default();

        // 要員構成(重複エンジニアは除く)
        let mut seen_engineers: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut staff_internal_count = 0usize;
        let mut staff_partner_count = 0usize;
        for r in &rows {
            if seen_engineers.insert(r.row.engineer_id) {
                if r.row.partner_contract_id.is_none() {
                    staff_internal_count += 1;
                } else {
                    staff_partner_count += 1;
                }
            }
        }

        let timesheet_total_count = rows.len();
        let timesheet_approved_count = rows.iter()
            .filter(|r| r.row.timesheet_status.as_deref() == Some("APPROVED"))
            .count();
        let pending_engineer_names: Vec<String> = rows.iter()
            .filter(|r| r.row.timesheet_status.as_deref() != Some("APPROVED")
                     && r.row.timesheet_status.as_deref() != Some("SENT"))
            .map(|r| r.row.engineer_name.clone())
            .collect();
        let is_finalized = rows.iter().all(|r| {
            r.row.timesheet_id.is_some() && r.row.timesheet_status.as_deref() != Some("PENDING")
        });

        let billing_total: i32 = rows.iter().map(|r| r.billing_amount).sum();
        let partner_cost_total: i32 = rows.iter().map(|r| r.payment_amount).sum();

        let internal_cost_total: i32 = rows.iter()
            .filter(|r| r.row.partner_contract_id.is_none())
            .map(|r| {
                let total_effort = engineer_effort_totals
                    .get(&r.row.engineer_id).copied().unwrap_or(r.row.billing_effort);
                if total_effort.is_zero() {
                    tracing::warn!(
                        "project_dashboard: total_effort=0 engineer_id={} project_id={}",
                        r.row.engineer_id, project_id
                    );
                    return 0;
                }
                let gross_pay = gross_pay_by_code
                    .get(&r.row.engineer_employee_code).copied()
                    .unwrap_or_else(|| {
                        tracing::warn!(
                            "project_dashboard: gross_pay not found employee_code={} month={}",
                            r.row.engineer_employee_code, target_month
                        );
                        0
                    });
                let ratio = r.row.billing_effort / total_effort;
                (Decimal::from(gross_pay) * ratio).round().to_i32().unwrap_or(0)
            })
            .sum();

        let profit = billing_total - partner_cost_total - internal_cost_total;

        ProjectSummary {
            project_id, project_name, client_name,
            staff_internal_count, staff_partner_count,
            timesheet_approved_count, timesheet_total_count,
            pending_engineer_names,
            billing_total, partner_cost_total, internal_cost_total, profit,
            is_finalized,
            engineers: rows,
        }
    }).collect();

    summaries.sort_by(|a, b| a.project_name.cmp(&b.project_name));
    Ok(summaries)
}
