//! 勤務表（メール添付）と受注の結び付け画面のためのデータ取得・取り込み処理
//!
//! 受領した勤務表が正。左に受注（取引先ごと・人・月・案件）、右にメールの勤務表を並べ、
//! 氏名（空白の違いは無視）と月で自動的に結び付け、決まらないものは人が結び付ける。
//! 案件名は入っていない・受注と違うことがあるため、結び付けの決め手にはせず、
//! 同じ人の受注が同じ月に複数あるときの絞り込みにだけ使う。

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;

use super::phase3_parse_register::{month_end_date, normalize_engineer_name_key};
use crate::domain::models::mail_pipeline::PhaseError;
use crate::infrastructure::repositories::order_repo;

// 勤務表を読み取り、氏名・対象月・契約まで照合した結果
struct ResolvedTimesheet {
    parsed: crate::domain::services::excel_parser::TimesheetParseResult,
    target_month: NaiveDate,
    engineer_name: String,
    contract_id: i64,
    settlement_overtime: Decimal,
}

/// 勤務表（Excel/PDF）を読み取り、対象月まで特定する（DBは更新しない）
fn parse_timesheet(
    attachment_filename: &str,
    raw_bytes: &[u8],
) -> Result<(crate::domain::services::excel_parser::TimesheetParseResult, NaiveDate), PhaseError> {
    let parsed = crate::domain::services::excel_parser::auto_detect_and_parse(raw_bytes, attachment_filename);
    if let Some(err) = &parsed.error {
        return Err(PhaseError::Permanent(format!("稼働報告解析失敗: {err}")));
    }
    let target_month = parsed
        .target_month
        .ok_or_else(|| PhaseError::Permanent("稼働報告から対象月を特定できませんでした".to_string()))?;
    Ok((parsed, target_month))
}

/// 手動で選ばれた受注から、結び付け先の契約と社員名を取り出す（対象月が違う受注は不可）
async fn contract_of_order(
    pool: &PgPool,
    order_id: i64,
    target_month: NaiveDate,
) -> Result<(i64, String), PhaseError> {
    let row: Option<(Option<i64>, NaiveDate, Option<String>)> = sqlx::query_as(
        r#"SELECT o.client_contract_id, o.target_month, e.name
           FROM t_received_order o
           LEFT JOIN m_engineer e ON e.id = o.engineer_id
           WHERE o.id = $1"#,
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("受注検索エラー: {e}")))?;
    let Some((contract_id, order_month, engineer_name)) = row else {
        return Err(PhaseError::Permanent("選ばれた受注が見つかりません".to_string()));
    };
    if order_month != target_month {
        return Err(PhaseError::Permanent(format!(
            "選ばれた受注の対象月（{}）が、勤務表の対象月（{}）と違います",
            order_month.format("%Y-%m"),
            target_month.format("%Y-%m")
        )));
    }
    let Some(contract_id) = contract_id else {
        return Err(PhaseError::Permanent("選ばれた受注に契約が紐づいていません".to_string()));
    };
    Ok((contract_id, engineer_name.unwrap_or_default()))
}

/// 契約が決まった勤務表から、精算超過時間などを計算する
async fn build_resolved(
    pool: &PgPool,
    parsed: crate::domain::services::excel_parser::TimesheetParseResult,
    target_month: NaiveDate,
    contract_id: i64,
    engineer_name: String,
) -> Result<ResolvedTimesheet, PhaseError> {
    use crate::domain::value_objects::SettlementTerms;
    let contract = order_repo::find_client_contract_for_order(pool, contract_id)
        .await
        .map_err(|e| PhaseError::Transient(format!("契約取得エラー: {e}")))?
        .ok_or_else(|| PhaseError::Permanent(format!("契約 id={contract_id} が見つかりません")))?;
    let terms = SettlementTerms {
        lower_limit_hours: contract.lower_limit_hours,
        upper_limit_hours: contract.upper_limit_hours,
        fixed_hours: contract.fixed_hours,
        deduction_rate: contract.deduction_rate,
        overtime_rate: contract.overtime_rate,
    };
    let settlement_overtime = terms.excess_hours(parsed.total_hours);
    Ok(ResolvedTimesheet { parsed, target_month, engineer_name, contract_id, settlement_overtime })
}

/// 同じ契約・対象月の既存の稼働報告（id, status）
async fn find_existing_timesheet(
    pool: &PgPool,
    contract_id: i64,
    target_month: NaiveDate,
) -> Result<Option<(i64, String)>, PhaseError> {
    sqlx::query_as(
        "SELECT id, status FROM t_monthly_timesheet WHERE client_contract_id = $1 AND target_month = $2",
    )
    .bind(contract_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("重複チェックエラー: {e}")))
}

/// 稼働報告として保存する。同月に承認前のものがあれば置き換え、無ければ新規登録する。戻り値は稼働報告ID。
///
/// 受領した勤務表が正。承認後は請求・支払に使われているため、置き換えない。
async fn save_timesheet(
    pool: &PgPool,
    attachment_filename: &str,
    email_id: i64,
    r: &ResolvedTimesheet,
) -> Result<i64, PhaseError> {
    let parsed = &r.parsed;
    let daily_data = serde_json::to_value(&parsed.daily_data).unwrap_or(serde_json::Value::Null);
    let alerts_json = serde_json::to_value(&parsed.alerts).unwrap_or(serde_json::Value::Null);

    if let Some((existing_id, existing_status)) = find_existing_timesheet(pool, r.contract_id, r.target_month).await? {
        if !matches!(existing_status.as_str(), "UPLOADED" | "PARSED" | "PENDING") {
            return Err(PhaseError::Permanent(format!(
                "{}分の稼働報告は承認済み等のため取り込めません（状態: {existing_status}）",
                r.target_month.format("%Y年%m月")
            )));
        }
        sqlx::query(
            r#"
            UPDATE t_monthly_timesheet
            SET status = 'UPLOADED', total_hours = $2, work_days = $3, overtime_hours = $4,
                daily_data = $5, original_filename = $6, alerts_json = $7,
                uploaded_at = NOW(), updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(existing_id)
        .bind(parsed.total_hours)
        .bind(parsed.work_days)
        .bind(r.settlement_overtime)
        .bind(&daily_data)
        .bind(attachment_filename)
        .bind(&alerts_json)
        .execute(pool)
        .await
        .map_err(|e| PhaseError::Transient(format!("稼働報告更新エラー: {e}")))?;
        tracing::info!(
            "[Phase3] 稼働報告を置き換え: timesheet_id={existing_id}, target_month={}, email_id={email_id}",
            r.target_month
        );
        return Ok(existing_id);
    }

    let new_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_monthly_timesheet (
            client_contract_id, target_month, status, total_hours, work_days,
            overtime_hours, night_hours, holiday_hours,
            daily_data, original_filename, alerts_json, uploaded_at
        ) VALUES ($1, $2, 'UPLOADED', $3, $4, $5, $6, $7, $8, $9, $10, NOW())
        RETURNING id
        "#,
    )
    .bind(r.contract_id)
    .bind(r.target_month)
    .bind(parsed.total_hours)
    .bind(parsed.work_days)
    .bind(r.settlement_overtime)
    .bind(Decimal::ZERO)
    .bind(Decimal::ZERO)
    .bind(&daily_data)
    .bind(attachment_filename)
    .bind(&alerts_json)
    .fetch_one(pool)
    .await
    .map_err(|e| PhaseError::Transient(format!("稼働報告INSERTエラー: {e}")))?;

    // 報告受領: 発注/受注を REPORT_RECEIVED へ（失敗しても稼働報告登録は成功扱い）
    match order_repo::advance_to_report_received_for_timesheet(pool, r.contract_id, r.target_month).await {
        Ok((po_n, ro_n)) if po_n > 0 || ro_n > 0 => {
            tracing::info!(
                "[Phase3] 報告受領ステータス更新: purchase_orders={po_n}, received_orders={ro_n}, contract_id={}, target_month={}",
                r.contract_id, r.target_month
            );
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(
                "[Phase3] 報告受領ステータス更新失敗（稼働報告は登録済み）: contract_id={}, target_month={}, err={e}",
                r.contract_id, r.target_month
            );
        }
    }

    tracing::info!(
        "[Phase3] 稼働報告登録完了: timesheet_id={new_id}, contract_id={}, target_month={}, worker={}",
        r.contract_id, r.target_month, parsed.worker_name
    );
    Ok(new_id)
}

// ============================================================
// 氏名・月による受注の提案（DBに依存しない純粋関数）
// ============================================================

/// 提案の候補にする受注（同じ月のもの）
#[derive(Debug, Clone)]
pub struct OrderKey {
    pub id: i64,
    pub engineer_name: String,
    pub project_name: String,
}

/// 勤務表の氏名が、社員マスタのいずれかと（空白の違いを無視して）一致するか。
/// 受注に結び付かない理由が、「その月の受注が未登録」なのか「氏名が社員と一致しない」のかを分けるために使う。
pub fn is_known_engineer(worker_name: &str, engineer_names: &[String]) -> bool {
    let key = normalize_engineer_name_key(worker_name);
    !key.is_empty() && engineer_names.iter().any(|n| normalize_engineer_name_key(n) == key)
}

fn normalize_project(name: &str) -> String {
    name.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect::<String>().to_lowercase()
}

/// 勤務表の氏名から、結び付ける受注を提案する。戻り値は（自動で決まる受注, 候補の受注ID一覧）。
///
/// - 氏名（空白の違いは無視）が一致する受注が1件なら、それに決める。
/// - 複数あるときだけ、案件名（勤務表にあれば）が一方を含む受注が1件に絞れれば、それに決める。
/// - 案件名が無い・違うことを理由に、候補から外したり結び付けを断ったりはしない。
pub fn suggest_order(worker_name: &str, project_name: &str, orders: &[OrderKey]) -> (Option<i64>, Vec<i64>) {
    let key = normalize_engineer_name_key(worker_name);
    if key.is_empty() {
        return (None, vec![]);
    }
    let candidates: Vec<&OrderKey> = orders
        .iter()
        .filter(|o| normalize_engineer_name_key(&o.engineer_name) == key)
        .collect();
    let ids: Vec<i64> = candidates.iter().map(|o| o.id).collect();
    match candidates.as_slice() {
        [] => (None, ids),
        [only] => (Some(only.id), ids),
        many => {
            let p = normalize_project(project_name);
            if p.is_empty() {
                return (None, ids);
            }
            let by_project: Vec<&&OrderKey> = many
                .iter()
                .filter(|o| {
                    let op = normalize_project(&o.project_name);
                    !op.is_empty() && (op.contains(&p) || p.contains(&op))
                })
                .collect();
            match by_project.as_slice() {
                [one] => (Some(one.id), ids),
                _ => (None, ids),
            }
        }
    }
}

// ============================================================
// 添付の読み出し・読み取り結果のキャッシュ
// ============================================================

struct AttachmentRow {
    email_id: i64,
    filename: String,
    content: Vec<u8>,
}

async fn load_attachment(pool: &PgPool, attachment_id: i64) -> Result<AttachmentRow, String> {
    let row: Option<(i64, String, Vec<u8>)> = sqlx::query_as(
        "SELECT email_id, filename, content FROM t_received_email_attachment WHERE id = $1",
    )
    .bind(attachment_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("添付の取得に失敗しました: {e}"))?;
    let (email_id, filename, content) = row.ok_or_else(|| "対象の添付が見つかりません".to_string())?;
    Ok(AttachmentRow { email_id, filename, content })
}

/// 添付の原本（ファイル名・バイト列）。取り込み確認画面の左側の表示用
pub async fn attachment_file(pool: &PgPool, attachment_id: i64) -> Result<(String, Vec<u8>), String> {
    let a = load_attachment(pool, attachment_id).await?;
    Ok((a.filename, a.content))
}

/// 勤務表の読み取り結果の要約（一覧用）。初回に読み取って、添付にキャッシュする
async fn parsed_summary(
    pool: &PgPool,
    attachment_id: i64,
    cached: Option<serde_json::Value>,
    filename: &str,
) -> serde_json::Value {
    if let Some(v) = cached {
        return v;
    }
    let content: Option<Vec<u8>> = sqlx::query_scalar("SELECT content FROM t_received_email_attachment WHERE id = $1")
        .bind(attachment_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let summary = match content {
        None => serde_json::json!({ "ok": false, "error": "添付を読み込めません" }),
        Some(bytes) => match parse_timesheet(filename, &bytes) {
            Ok((p, month)) => serde_json::json!({
                "ok": true,
                "worker_name": p.worker_name,
                "project_name": p.project_name,
                "target_month": month.format("%Y-%m").to_string(),
                "total_hours": p.total_hours,
                "work_days": p.work_days,
            }),
            Err(e) => serde_json::json!({
                "ok": false,
                "error": crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e),
            }),
        },
    };
    let _ = sqlx::query("UPDATE t_received_email_attachment SET parsed_json = $2 WHERE id = $1")
        .bind(attachment_id)
        .bind(&summary)
        .execute(pool)
        .await;
    summary
}

// ============================================================
// 結び付け画面のデータ
// ============================================================

fn looks_like_timesheet_hint(filename: &str, subject: &str) -> bool {
    let t = format!("{filename} {subject}");
    ["勤務表", "作業報告", "稼働", "実績", "月末"].iter().any(|k| t.contains(k))
}

/// 指定月の結び付け画面のデータ（左: 取引先ごとの受注、右: 届いた勤務表と提案）
pub async fn board(pool: &PgPool, month: NaiveDate) -> Result<serde_json::Value, String> {
    // ── 左: 受注 ──
    let order_rows: Vec<(i64, i64, String, Option<String>, String, String, Option<i64>, Option<i64>, Option<String>, Option<f64>)> =
        sqlx::query_as(
            r#"SELECT o.id, o.client_id, c.name, e.name, o.project_name, o.status, o.client_contract_id,
                      t.id, t.status, t.total_hours::float8
               FROM t_received_order o
               JOIN m_client c ON c.id = o.client_id
               LEFT JOIN m_engineer e ON e.id = o.engineer_id
               LEFT JOIN t_monthly_timesheet t
                      ON t.client_contract_id = o.client_contract_id AND t.target_month = o.target_month
               WHERE o.target_month = $1
               ORDER BY c.name, e.name, o.id"#,
        )
        .bind(month)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("受注の取得に失敗しました: {e}"))?;

    let engineer_names: Vec<String> = sqlx::query_scalar("SELECT name FROM m_engineer")
        .fetch_all(pool)
        .await
        .map_err(|e| format!("社員の取得に失敗しました: {e}"))?;

    let keys: Vec<OrderKey> = order_rows
        .iter()
        .map(|r| OrderKey { id: r.0, engineer_name: r.3.clone().unwrap_or_default(), project_name: r.4.clone() })
        .collect();

    // ── 右: 勤務表の添付 ──
    let window_start = month - chrono::Duration::days(15);
    let window_end = month_end_date(month) + chrono::Duration::days(75);
    #[allow(clippy::type_complexity)]
    let att_rows: Vec<(i64, i64, String, Option<serde_json::Value>, Option<i64>, String, String, String,
                       chrono::DateTime<chrono::Utc>, Option<String>, Option<String>, Option<String>, String)> = sqlx::query_as(
        r#"SELECT a.id, a.email_id, a.filename, a.parsed_json, a.imported_timesheet_id,
                  e.from_name, e.from_email, e.subject, e.received_at, c.name, p.name,
                  a.review_status, a.rejected_reason
           FROM t_received_email_attachment a
           JOIN t_received_email e ON e.id = a.email_id
           LEFT JOIN m_client c ON c.id = e.client_id
           LEFT JOIN m_partner p ON p.partner_id = e.partner_id
           WHERE e.received_at >= $1::date AND e.received_at < $2::date
           ORDER BY e.received_at DESC, a.seq"#,
    )
    .bind(window_start)
    .bind(window_end)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("添付の取得に失敗しました: {e}"))?;

    let month_str = month.format("%Y-%m").to_string();
    let mut attachments: Vec<serde_json::Value> = Vec::new();
    // 1つの受注に提案できる勤務表は、最後に届いた1件だけ（前のものは「差し替え前」）
    let mut suggested_owner: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    for (id, email_id, filename, cached, imported_ts, from_name, from_email, subject, received_at, client_name, partner_name, review_status, rejected_reason) in att_rows {
        let rejected = review_status.as_deref() == Some("REJECTED");
        let summary = parsed_summary(pool, id, cached, &filename).await;
        let ok = summary["ok"].as_bool().unwrap_or(false);
        if ok {
            if summary["target_month"].as_str() != Some(month_str.as_str()) {
                continue;
            }
        } else if !looks_like_timesheet_hint(&filename, &subject) {
            continue;
        }

        let (suggested, candidate_ids) = if ok && imported_ts.is_none() && !rejected {
            suggest_order(
                summary["worker_name"].as_str().unwrap_or(""),
                summary["project_name"].as_str().unwrap_or(""),
                &keys,
            )
        } else {
            (None, vec![])
        };
        let mut superseded = false;
        if let Some(oid) = suggested {
            if suggested_owner.contains_key(&oid) {
                superseded = true; // 新しい勤務表が既に提案されている
            } else {
                suggested_owner.insert(oid, id);
            }
        }
        let (sender_kind, sender_name) = match (&client_name, &partner_name) {
            (Some(n), _) => ("client", n.clone()),
            (None, Some(n)) => ("partner", n.clone()),
            _ => ("unknown", String::new()),
        };
        attachments.push(serde_json::json!({
            "attachment_id": id,
            "email_id": email_id,
            "filename": filename,
            "subject": subject,
            "from_name": from_name,
            "from_email": from_email,
            "received_at": received_at.to_rfc3339(),
            "sender_kind": sender_kind,
            "sender_name": sender_name,
            "parsed": summary,
            "imported_timesheet_id": imported_ts,
            "suggested_order_id": if superseded { None } else { suggested },
            "candidate_order_ids": candidate_ids,
            // 氏名が社員マスタにあるか（無ければ表記の違い、あればその月の受注が未登録）
            "engineer_known": ok && is_known_engineer(summary["worker_name"].as_str().unwrap_or(""), &engineer_names),
            "superseded": superseded,
            "rejected": rejected,
            "rejected_reason": rejected_reason,
        }));
    }

    let orders: Vec<serde_json::Value> = order_rows
        .into_iter()
        .map(|(id, client_id, client_name, engineer, project, status, contract_id, ts_id, ts_status, ts_hours)| {
            serde_json::json!({
                "order_id": id,
                "client_id": client_id,
                "client_name": client_name,
                "engineer_name": engineer.unwrap_or_default(),
                "project_name": project,
                "order_status": status,
                "client_contract_id": contract_id,
                "can_link": contract_id.is_some(),
                "timesheet_id": ts_id,
                "timesheet_status": ts_status,
                "timesheet_hours": ts_hours,
            })
        })
        .collect();

    Ok(serde_json::json!({ "month": month_str, "orders": orders, "attachments": attachments }))
}

// ============================================================
// 取り込み確認・取り込み（添付 × 受注）
// ============================================================

/// 取り込み確認画面用（左に原本、右に読み取り結果）。結び付ける受注を指定する。DBは更新しない
pub async fn preview_attachment(pool: &PgPool, attachment_id: i64, order_id: i64) -> Result<serde_json::Value, String> {
    let a = load_attachment(pool, attachment_id).await?;
    let (parsed, target_month) = parse_timesheet(&a.filename, &a.content)
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;
    let (contract_id, engineer_name) = contract_of_order(pool, order_id, target_month)
        .await
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;
    let order: Option<(String, String)> = sqlx::query_as(
        "SELECT c.name, o.project_name FROM t_received_order o JOIN m_client c ON c.id = o.client_id WHERE o.id = $1",
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("受注の取得に失敗しました: {e}"))?;
    let (client_name, order_project) = order.unwrap_or_default();
    // 差し戻すときの依頼文に使う、送信元の情報
    let mail: Option<(String, String, String)> = sqlx::query_as(
        "SELECT from_name, from_email, subject FROM t_received_email WHERE id = $1",
    )
    .bind(a.email_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("メールの取得に失敗しました: {e}"))?;
    let (from_name, from_email, subject) = mail.unwrap_or_default();
    let existing = find_existing_timesheet(pool, contract_id, target_month)
        .await
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;
    let (existing_status, can_import) = match &existing {
        Some((_, st)) => (Some(st.clone()), matches!(st.as_str(), "UPLOADED" | "PARSED" | "PENDING")),
        None => (None, true),
    };
    let name_matches = normalize_engineer_name_key(&parsed.worker_name) == normalize_engineer_name_key(&engineer_name);
    let project_matches = if parsed.project_name.trim().is_empty() || order_project.trim().is_empty() {
        None
    } else {
        let (a, b) = (normalize_project(&parsed.project_name), normalize_project(&order_project));
        Some(a.contains(&b) || b.contains(&a))
    };
    // 祝日（画面で、土日と同じように背景色を変えて表示する）
    let holiday_dates: Vec<String> = parsed
        .daily_data
        .iter()
        .filter(|d| {
            NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
                .map(crate::domain::services::business_day::is_national_holiday)
                .unwrap_or(false)
        })
        .map(|d| d.date.clone())
        .collect();
    Ok(serde_json::json!({
        "email": { "from_name": from_name, "from_email": from_email, "subject": subject },
        "filename": a.filename,
        "worker_name": parsed.worker_name,
        "project_name": parsed.project_name,
        "target_month": target_month.format("%Y-%m").to_string(),
        "total_hours": parsed.total_hours,
        "work_days": parsed.work_days,
        "daily_data": parsed.daily_data,
        "holiday_dates": holiday_dates,
        "alerts": parsed.alerts,
        "order": {
            "order_id": order_id,
            "client_name": client_name,
            "engineer_name": engineer_name,
            "project_name": order_project,
        },
        "name_matches": name_matches,
        "project_matches": project_matches,
        "existing_status": existing_status,
        "can_import": can_import,
    }))
}

/// 添付の勤務表を、指定した受注の稼働報告として取り込む（承認前の同月の稼働報告があれば置き換える）。
/// 戻り値は稼働報告ID。承認は呼び出し側（ハンドラ）が続けて行う。
pub async fn import_attachment(pool: &PgPool, attachment_id: i64, order_id: i64) -> Result<i64, String> {
    let a = load_attachment(pool, attachment_id).await?;
    let (parsed, target_month) = parse_timesheet(&a.filename, &a.content)
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;
    let (contract_id, engineer_name) = contract_of_order(pool, order_id, target_month)
        .await
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;
    let resolved = build_resolved(pool, parsed, target_month, contract_id, engineer_name)
        .await
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;
    let timesheet_id = save_timesheet(pool, &a.filename, a.email_id, &resolved)
        .await
        .map_err(|e| crate::domain::models::mail_pipeline::tag_phase_error("Phase3", &e))?;

    sqlx::query(
        "UPDATE t_received_email_attachment SET imported_timesheet_id = $2, review_status = NULL, rejected_reason = '', rejected_at = NULL WHERE id = $1",
    )
        .bind(attachment_id)
        .bind(timesheet_id)
        .execute(pool)
        .await
        .map_err(|e| format!("添付の状態更新に失敗しました: {e}"))?;
    // メール内の書類がすべて取り込み済みになったら、メールも取込済・要確認なしにする
    sqlx::query(
        r#"UPDATE t_received_email
           SET status = 'IMPORTED', needs_manual_review = FALSE, error_message = '', processed_at = NOW()
           WHERE id = $1
             AND NOT EXISTS (SELECT 1 FROM t_received_email_attachment
                             WHERE email_id = $1 AND imported_timesheet_id IS NULL
                               AND review_status IS DISTINCT FROM 'REJECTED')"#,
    )
    .bind(a.email_id)
    .execute(pool)
    .await
    .map_err(|e| format!("メール状態の更新に失敗しました: {e}"))?;
    Ok(timesheet_id)
}

/// 勤務表を差し戻す（承認せずに、送信元へ再提出を依頼する）。理由と日時を残し、取り込み待ちの一覧から外す。
/// 取り込み済みの勤務表は差し戻せない。メールの送信はしない（依頼文は画面でコピーして、人が送る）。
pub async fn reject_attachment(pool: &PgPool, attachment_id: i64, reason: &str) -> Result<(), String> {
    let a = load_attachment(pool, attachment_id).await?;
    let updated = sqlx::query(
        r#"UPDATE t_received_email_attachment
           SET review_status = 'REJECTED', rejected_reason = $2, rejected_at = NOW()
           WHERE id = $1 AND imported_timesheet_id IS NULL"#,
    )
    .bind(attachment_id)
    .bind(reason.trim())
    .execute(pool)
    .await
    .map_err(|e| format!("差し戻しに失敗しました: {e}"))?
    .rows_affected();
    if updated == 0 {
        return Err("取り込み済みの勤務表は差し戻せません".to_string());
    }
    // メール内の勤務表がすべて取り込み済みか差し戻し済みになったら、メールの「要確認」を外す。
    // status は SKIPPED（終端）にして、Phase3 が再処理して要確認に戻すことがないようにする
    sqlx::query(
        r#"UPDATE t_received_email
           SET status = 'SKIPPED', needs_manual_review = FALSE, error_message = '勤務表を差し戻しました', processed_at = NOW()
           WHERE id = $1
             AND NOT EXISTS (SELECT 1 FROM t_received_email_attachment
                             WHERE email_id = $1 AND imported_timesheet_id IS NULL
                               AND review_status IS DISTINCT FROM 'REJECTED')"#,
    )
    .bind(a.email_id)
    .execute(pool)
    .await
    .map_err(|e| format!("メール状態の更新に失敗しました: {e}"))?;
    Ok(())
}

/// 差し戻しを取り消して、取り込み待ちに戻す
pub async fn unreject_attachment(pool: &PgPool, attachment_id: i64) -> Result<(), String> {
    let a = load_attachment(pool, attachment_id).await?;
    sqlx::query(
        "UPDATE t_received_email_attachment SET review_status = NULL, rejected_reason = '', rejected_at = NULL WHERE id = $1",
    )
    .bind(attachment_id)
    .execute(pool)
    .await
    .map_err(|e| format!("差し戻しの取り消しに失敗しました: {e}"))?;
    sqlx::query(
        r#"UPDATE t_received_email
           SET status = 'FETCHED', needs_manual_review = TRUE, error_message = '勤務表: 内容を確認して取り込んでください'
           WHERE id = $1 AND status = 'SKIPPED'"#,
    )
    .bind(a.email_id)
    .execute(pool)
    .await
    .map_err(|e| format!("メール状態の更新に失敗しました: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(id: i64, name: &str, project: &str) -> OrderKey {
        OrderKey { id, engineer_name: name.to_string(), project_name: project.to_string() }
    }

    #[test]
    fn suggests_the_only_order_of_the_same_person() {
        let orders = vec![o(1, "鈴木花子", "イオン保険サービス"), o(2, "日高 直樹", "テスト案件")];
        assert_eq!(suggest_order("鈴木 花子", "", &orders), (Some(1), vec![1]));
    }

    #[test]
    fn ignores_space_differences_including_fullwidth() {
        let orders = vec![o(1, "日高 直樹", "A")];
        assert_eq!(suggest_order("日高\u{3000}直樹", "", &orders).0, Some(1));
        assert_eq!(suggest_order("日高直樹", "", &orders).0, Some(1));
    }

    #[test]
    fn does_not_reject_when_project_name_differs_or_is_missing() {
        let orders = vec![o(1, "鈴木花子", "イオン保険サービス")];
        assert_eq!(suggest_order("鈴木花子", "全然違う案件名", &orders).0, Some(1));
        assert_eq!(suggest_order("鈴木花子", "", &orders).0, Some(1));
    }

    #[test]
    fn uses_project_only_to_choose_among_several_orders() {
        let orders = vec![o(1, "鈴木花子", "イオン保険サービス"), o(2, "鈴木花子", "別案件PM支援")];
        assert_eq!(suggest_order("鈴木花子", "PM支援", &orders), (Some(2), vec![1, 2]));
        // 案件名が無い・どちらにも合わない → 自動では決めず、候補だけ返す
        assert_eq!(suggest_order("鈴木花子", "", &orders), (None, vec![1, 2]));
        assert_eq!(suggest_order("鈴木花子", "無関係", &orders), (None, vec![1, 2]));
    }

    #[test]
    fn tells_unregistered_order_from_unknown_name() {
        let names = vec!["鈴木花子".to_string(), "日高 直樹".to_string()];
        assert!(is_known_engineer("鈴木 花子", &names));
        assert!(is_known_engineer("日高\u{3000}直樹", &names));
        assert!(!is_known_engineer("佐藤ゆたか", &names));
        assert!(!is_known_engineer("", &names));
    }

    #[test]
    fn returns_nothing_for_unknown_or_empty_name() {
        let orders = vec![o(1, "鈴木花子", "A")];
        assert_eq!(suggest_order("佐藤ゆたか", "", &orders), (None, vec![]));
        assert_eq!(suggest_order("  ", "", &orders), (None, vec![]));
    }
}
