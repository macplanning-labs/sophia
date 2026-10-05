//! 請求書の確定(UI刷新 2-4 / DEMO-000136)。
//!
//! プレビュー(`billing_preview`)で見せた内容を、**取引先×月の排他ロックの中で再計算してから**、
//! 請求書として1回だけ作る。揃っていないときは、理由つきの強制確定だけを許し、履歴を残す。
//! 作成の本体は旧画面の一括発行と同じ `issue_invoices_in_tx`(金額・番号・集約が同じ)。

use std::collections::{BTreeMap, HashMap};

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::domain::services::billing_preview::{build_previews, InvoicePreview, MissingItem};
use crate::domain::services::settlement_dashboard::{
    calculate_preview, issue_invoices_in_tx, list_settlement_rows, verify_issued_invoices, BillingUnit,
    SettlementFilter, SettlementViewRow,
};
use crate::infrastructure::db_tx::commit_checked;
use crate::infrastructure::repositories::{settlement_repo, tax_rate_repo};

/// 遅れている契約の扱い
pub const FORCE_ACTIONS: [&str; 2] = ["NEXT_MONTH", "SECOND_INVOICE"];
const MAX_KEYS: usize = 50;
const MAX_REASON_CHARS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct ForceAction {
    pub client_contract_id: i64,
    /// "NEXT_MONTH"(翌月回し) | "SECOND_INVOICE"(当月2通目で後日発行)
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConfirmRequest {
    /// "YYYY-MM"
    pub month: String,
    /// プレビューの行ID(`c{取引先}` / `c{取引先}:p{案件}`)
    pub keys: Vec<String>,
    #[serde(default)]
    pub force: bool,
    #[serde(default)]
    pub force_reason: Option<String>,
    #[serde(default)]
    pub force_actions: Vec<ForceAction>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfirmedInvoice {
    pub key: String,
    pub invoice_id: i64,
    pub invoice_no: String,
    pub total: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedKey {
    pub key: String,
    /// NOT_FOUND | EDI_EXCLUDED | NOTHING_TO_ISSUE | NOT_CONFIRMABLE
    pub reason: String,
    pub missing: Vec<MissingItem>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ConfirmOutcome {
    pub created: Vec<ConfirmedInvoice>,
    pub skipped: Vec<SkippedKey>,
}

#[derive(Debug)]
pub enum ConfirmError {
    /// 入力が不正(400)
    BadRequest(String),
    /// 内部エラー(500)。全体を取り消した
    Internal(String),
}

/// "YYYY-MM" または "YYYY-MM-DD" を、その月の1日にする
pub fn parse_month(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{}-01", s.trim().get(..7)?), "%Y-%m-%d").ok()
}

/// 行ID から取引先IDを取り出す
pub fn client_id_of_key(key: &str) -> Option<i64> {
    let head = key.split(':').next()?;
    head.strip_prefix('c')?.parse().ok()
}

/// 入力の検証(DBを使わない部分)。対象月を返す
pub fn validate_request(req: &ConfirmRequest) -> Result<NaiveDate, ConfirmError> {
    let month = parse_month(&req.month)
        .ok_or_else(|| ConfirmError::BadRequest("対象月は YYYY-MM の形で指定してください".into()))?;
    if req.keys.is_empty() {
        return Err(ConfirmError::BadRequest("確定する請求書が選択されていません".into()));
    }
    if req.keys.len() > MAX_KEYS {
        return Err(ConfirmError::BadRequest(format!("一度に確定できるのは{MAX_KEYS}件までです")));
    }
    if req.force {
        let reason = req.force_reason.as_deref().unwrap_or("").trim();
        if reason.is_empty() {
            return Err(ConfirmError::BadRequest("強制確定には理由が必要です".into()));
        }
        if reason.chars().count() > MAX_REASON_CHARS {
            return Err(ConfirmError::BadRequest(format!("理由は{MAX_REASON_CHARS}文字以内で入力してください")));
        }
    }
    for fa in &req.force_actions {
        if !FORCE_ACTIONS.contains(&fa.action.as_str()) {
            return Err(ConfirmError::BadRequest(format!(
                "遅れている契約の扱いが不正です: {}(NEXT_MONTH か SECOND_INVOICE)", fa.action
            )));
        }
    }
    Ok(month)
}

/// 強制確定で、揃っていない契約すべてに扱いが指定されているか。指定のない契約IDを返す
pub fn uncovered_missing(missing: &[MissingItem], actions: &[ForceAction]) -> Vec<i64> {
    missing
        .iter()
        .filter(|m| !actions.iter().any(|a| a.client_contract_id == m.client_contract_id))
        .map(|m| m.client_contract_id)
        .collect()
}

/// 取引先1社分の月次確定の行と、プレビューを読み込む
pub async fn load_previews(
    pool: &PgPool,
    month: NaiveDate,
    client_id: Option<i64>,
) -> Result<(Vec<SettlementViewRow>, Vec<InvoicePreview>), String> {
    let filter = SettlementFilter { client_id, ..SettlementFilter::default() };
    let rows = list_settlement_rows(pool, month, &filter).await.map_err(|e| e.to_string())?;
    let (view_rows, _) = calculate_preview(rows);

    let mut client_ids: Vec<i64> = view_rows.iter().map(|v| v.row.client_id).collect();
    client_ids.sort_unstable();
    client_ids.dedup();
    let units: HashMap<i64, BillingUnit> = settlement_repo::client_billing_units(pool, &client_ids)
        .await
        .map_err(|e| format!("請求単位の取得に失敗しました: {e}"))?
        .into_iter()
        .map(|(id, s)| (id, BillingUnit::from_db(&s)))
        .collect();
    let tax_rate = tax_rate_repo::find_effective_rate(pool, month).await.unwrap_or(Decimal::from(10));
    let previews = build_previews(&view_rows, &units, tax_rate, month);
    Ok((view_rows, previews))
}

/// 請求書を確定する。取引先ごとに1つのトランザクション+排他ロックで処理する。
pub async fn confirm_invoices(
    pool: &PgPool,
    user_id: i64,
    req: &ConfirmRequest,
) -> Result<ConfirmOutcome, ConfirmError> {
    let month = validate_request(req)?;
    let ym = format!("{:04}{:02}", month.year(), month.month());
    let internal = |e: String| ConfirmError::Internal(e);

    let mut outcome = ConfirmOutcome::default();
    // 取引先ごとにまとめる(取引先IDの順=ロックの取得順を一定にして、デッドロックを避ける)
    let mut by_client: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for key in &req.keys {
        match client_id_of_key(key) {
            Some(c) => by_client.entry(c).or_default().push(key.clone()),
            None => outcome.skipped.push(SkippedKey { key: key.clone(), reason: "NOT_FOUND".into(), missing: vec![] }),
        }
    }

    for (client_id, keys) in by_client {
        let mut tx = pool.begin().await.map_err(|e| internal(format!("TX error: {e}")))?;
        // 取引先×月で直列化。後から来た側は、先の確定(コミット済み)を見て再計算する
        settlement_repo::lock_numbering(&mut tx, &format!("billing:{client_id}:{ym}"))
            .await
            .map_err(|e| internal(format!("排他ロックに失敗しました: {e}")))?;

        let (view_rows, previews) = load_previews(pool, month, Some(client_id)).await.map_err(internal)?;
        let mut issued: Vec<(String, crate::domain::services::settlement_dashboard::IssuedInvoice, Vec<MissingItem>)> = Vec::new();

        for key in keys {
            let Some(p) = previews.iter().find(|p| p.key == key) else {
                outcome.skipped.push(SkippedKey { key, reason: "NOT_FOUND".into(), missing: vec![] });
                continue;
            };
            if p.edi_excluded {
                outcome.skipped.push(SkippedKey { key, reason: "EDI_EXCLUDED".into(), missing: vec![] });
                continue;
            }
            if p.items.is_empty() {
                outcome.skipped.push(SkippedKey { key, reason: "NOTHING_TO_ISSUE".into(), missing: vec![] });
                continue;
            }
            let mut forced_missing: Vec<MissingItem> = Vec::new();
            if !p.confirmable {
                if !req.force {
                    outcome.skipped.push(SkippedKey { key, reason: "NOT_CONFIRMABLE".into(), missing: p.missing.clone() });
                    continue;
                }
                let uncovered = uncovered_missing(&p.missing, &req.force_actions);
                if !uncovered.is_empty() {
                    return Err(ConfirmError::BadRequest(format!(
                        "遅れている契約の扱い(翌月回し/当月2通目)が指定されていません: 契約ID {uncovered:?}"
                    )));
                }
                forced_missing = p.missing.clone();
            }

            let contract_ids: Vec<i64> = p.items.iter().map(|i| i.client_contract_id).collect();
            let rows_for_key: Vec<SettlementViewRow> = view_rows
                .iter()
                .filter(|v| contract_ids.contains(&v.row.client_contract_id))
                .cloned()
                .collect();
            let mut made = issue_invoices_in_tx(pool, &mut tx, &rows_for_key, month).await.map_err(internal)?;
            if made.len() != 1 {
                return Err(ConfirmError::Internal(format!(
                    "請求書の作成数が想定と異なります: key={key} 作成数={}", made.len()
                )));
            }
            issued.push((key, made.remove(0), forced_missing));
        }

        // 強制確定の履歴(同じトランザクション)
        let reason = req.force_reason.as_deref().unwrap_or("").trim().to_string();
        for (_, inv, forced_missing) in &issued {
            for m in forced_missing {
                let action = req
                    .force_actions
                    .iter()
                    .find(|a| a.client_contract_id == m.client_contract_id)
                    .map(|a| a.action.as_str())
                    .unwrap_or("NEXT_MONTH");
                settlement_repo::insert_force_confirm(&mut tx, inv.id, m.client_contract_id, action, &reason, user_id)
                    .await
                    .map_err(|e| internal(format!("強制確定の履歴の保存に失敗しました: {e}")))?;
            }
        }

        commit_checked(tx).await.map_err(|e| internal(format!("トランザクション確定エラー: {e}")))?;

        let keys_in_order: Vec<String> = issued.iter().map(|(k, _, _)| k.clone()).collect();
        let created = verify_issued_invoices(pool, issued.into_iter().map(|(_, i, _)| i).collect())
            .await
            .map_err(internal)?;
        for (key, c) in keys_in_order.into_iter().zip(created) {
            outcome.created.push(ConfirmedInvoice { key, invoice_id: c.id, invoice_no: c.invoice_no, total: c.total });
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(month: &str, keys: &[&str]) -> ConfirmRequest {
        ConfirmRequest {
            month: month.into(),
            keys: keys.iter().map(|s| s.to_string()).collect(),
            force: false,
            force_reason: None,
            force_actions: vec![],
        }
    }

    #[test]
    fn parse_month_accepts_ym_and_ymd() {
        assert_eq!(parse_month("2026-10"), NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(parse_month("2026-10-15"), NaiveDate::from_ymd_opt(2026, 10, 1));
        assert_eq!(parse_month("2026-13"), None);
        assert_eq!(parse_month("abc"), None);
        assert_eq!(parse_month(""), None);
    }

    #[test]
    fn key_to_client_id() {
        assert_eq!(client_id_of_key("c12"), Some(12));
        assert_eq!(client_id_of_key("c12:pPRJ00000003"), Some(12));
        assert_eq!(client_id_of_key("x12"), None);
        assert_eq!(client_id_of_key("c"), None);
        assert_eq!(client_id_of_key(""), None);
    }

    #[test]
    fn validation_rejects_bad_input() {
        assert!(matches!(validate_request(&req("bad", &["c1"])), Err(ConfirmError::BadRequest(_))));
        assert!(matches!(validate_request(&req("2026-10", &[])), Err(ConfirmError::BadRequest(_))));
        let many: Vec<String> = (0..51).map(|i| format!("c{i}")).collect();
        let r = ConfirmRequest { month: "2026-10".into(), keys: many, force: false, force_reason: None, force_actions: vec![] };
        assert!(matches!(validate_request(&r), Err(ConfirmError::BadRequest(_))));
        assert!(validate_request(&req("2026-10", &["c1"])).is_ok());
    }

    #[test]
    fn force_requires_a_reason() {
        let mut r = req("2026-10", &["c1"]);
        r.force = true;
        assert!(matches!(validate_request(&r), Err(ConfirmError::BadRequest(_))), "理由なし");
        r.force_reason = Some("   ".into());
        assert!(matches!(validate_request(&r), Err(ConfirmError::BadRequest(_))), "空白のみ");
        r.force_reason = Some("先方都合で1名が遅れるため".into());
        assert!(validate_request(&r).is_ok());
        r.force_reason = Some("あ".repeat(501));
        assert!(matches!(validate_request(&r), Err(ConfirmError::BadRequest(_))), "501文字");
    }

    #[test]
    fn force_action_values_are_checked() {
        let mut r = req("2026-10", &["c1"]);
        r.force_actions = vec![ForceAction { client_contract_id: 1, action: "DELETE".into() }];
        assert!(matches!(validate_request(&r), Err(ConfirmError::BadRequest(_))));
        r.force_actions = vec![ForceAction { client_contract_id: 1, action: "SECOND_INVOICE".into() }];
        assert!(validate_request(&r).is_ok());
    }

    #[test]
    fn every_missing_contract_needs_an_action() {
        let missing = vec![
            MissingItem { client_contract_id: 1, engineer_name: "a".into(), project_id: "P".into(), project_name: "P".into(), timesheet_status: None },
            MissingItem { client_contract_id: 2, engineer_name: "b".into(), project_id: "P".into(), project_name: "P".into(), timesheet_status: None },
        ];
        let actions = vec![ForceAction { client_contract_id: 1, action: "NEXT_MONTH".into() }];
        assert_eq!(uncovered_missing(&missing, &actions), vec![2]);
        let all = vec![actions[0].clone(), ForceAction { client_contract_id: 2, action: "SECOND_INVOICE".into() }];
        assert!(uncovered_missing(&missing, &all).is_empty());
    }
}
