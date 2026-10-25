//! 勤務表の承認直後に返す「請求書の確認ヒント」(UI刷新 3-3a / DEMO-000142)。
//!
//! 承認した勤務表が入る請求書のプレビューから、あと何名で確定できるかだけを取り出す。
//! 承認の経路(管理者承認・取り込み確認)が複数あるため、ここに1つの関数を置いて共通で呼ぶ。
//! 失敗しても承認の結果には影響させない(ヒントが無いだけ)。

use chrono::NaiveDate;
use serde::Serialize;
use sqlx::PgPool;

use crate::domain::services::billing_confirm::load_previews;
use crate::domain::services::billing_preview::InvoicePreview;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BillingHint {
    pub client_id: i64,
    /// 請求書プレビューの行ID(`/billing?key=` に使う)
    pub key: String,
    /// 月("YYYY-MM")
    pub month: String,
    /// 揃っていない要員の数
    pub missing_count: usize,
    pub confirmable: bool,
}

/// プレビューの中から、指定の勤務表が明細に入っている請求書を選ぶ。
/// 明細に無い(請求書の対象外・すでに別の請求書に入っている等)ときは None。
pub fn pick_hint(previews: &[InvoicePreview], timesheet_id: i64, month: NaiveDate) -> Option<BillingHint> {
    previews
        .iter()
        .find(|p| p.items.iter().any(|i| i.timesheet_id == Some(timesheet_id)))
        .map(|p| BillingHint {
            client_id: p.client_id,
            key: p.key.clone(),
            month: month.format("%Y-%m").to_string(),
            missing_count: p.missing.len(),
            confirmable: p.confirmable,
        })
}

/// 勤務表の承認後に呼ぶ。案件に紐づかない勤務表(社員自己申告)や、計算に失敗したときは None。
pub async fn billing_hint_for_timesheet(pool: &PgPool, timesheet_id: i64) -> Option<BillingHint> {
    let row: Option<(NaiveDate, i64)> = sqlx::query_as(
        "SELECT t.target_month, prj.client_id
           FROM t_monthly_timesheet t
           JOIN m_client_contract cc ON cc.id = t.client_contract_id
           JOIN m_project prj ON prj.project_id = cc.project_id
          WHERE t.id = $1",
    )
    .bind(timesheet_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    let (month, client_id) = row?;
    match load_previews(pool, month, Some(client_id)).await {
        Ok((_, previews)) => pick_hint(&previews, timesheet_id, month),
        Err(e) => {
            tracing::warn!("billing_hint: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::billing_preview::{MissingItem, PreviewItem};

    fn preview(key: &str, ts: &[i64], missing: usize, confirmable: bool) -> InvoicePreview {
        InvoicePreview {
            key: key.into(),
            client_id: 7,
            client_name: "c".into(),
            billing_unit: "CLIENT".into(),
            project_id: None,
            subject: "s".into(),
            items: ts
                .iter()
                .map(|t| PreviewItem {
                    client_contract_id: 1,
                    engineer_id: 1,
                    engineer_name: "e".into(),
                    project_id: "p".into(),
                    project_name: "p".into(),
                    timesheet_id: Some(*t),
                    amount: 100,
                })
                .collect(),
            subtotal: 0,
            tax_amount: 0,
            total: 0,
            missing: (0..missing)
                .map(|i| MissingItem {
                    client_contract_id: i as i64,
                    engineer_name: "m".into(),
                    project_id: "p".into(),
                    project_name: "p".into(),
                    timesheet_status: None,
                })
                .collect(),
            already_issued_count: 0,
            confirmable,
            edi_excluded: false,
        }
    }

    #[test]
    fn picks_the_invoice_that_contains_the_timesheet() {
        let m = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        let ps = vec![preview("c7:pA", &[1], 0, true), preview("c7:pB", &[2, 3], 1, false)];
        let h = pick_hint(&ps, 3, m).unwrap();
        assert_eq!((h.key.as_str(), h.missing_count, h.confirmable), ("c7:pB", 1, false));
        assert_eq!(h.month, "2026-10");
    }

    #[test]
    fn none_when_timesheet_is_in_no_invoice() {
        let m = NaiveDate::from_ymd_opt(2026, 10, 1).unwrap();
        assert!(pick_hint(&[preview("c7", &[1], 0, true)], 99, m).is_none());
    }
}
