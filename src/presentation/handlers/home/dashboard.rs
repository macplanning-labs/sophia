/// home/dashboard.rs — 進捗集計・期限計算・ステータス変更・ダッシュボードAPI

use axum::{
    extract::{Extension, State, Path, Json},
    response::IntoResponse,
    http::StatusCode,
};
use sqlx::PgPool;
use serde::{Deserialize, Serialize};
use chrono::{NaiveDate, Datelike, Local};

use crate::presentation::middleware::role::AuthUser;
use crate::domain::services::business_day::{
    subtract_business_days, roll_to_business_day, resolve_report_deadline_settings, calculate_deadline,
};

use super::StatusResponse;
use super::mail::{build_mail_logs, count_confirmed_mails, count_needs_review_mails, count_fetch_failed_mails, mail_sync_last_processed_at, MailRow};

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ステータス定義
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 発注ステップ: 発注書 → 送付 → 承諾 → 報告 → 請求書 → 受諾 → 支払
const PO_STEPS: &[(&str, &str)] = &[
    ("DRAFT", "発注書"),
    ("SENT", "送付"),
    ("ACCEPTED", "承諾"),
    ("REPORT_RECEIVED", "報告"),
    ("NOTICE_CREATED", "請求書"),
    ("NOTICE_CONFIRMED", "受諾"),
    ("PAID", "支払"),
];

/// 受注ステップ: 受注 → 勤怠 → 送付 → 請求書 → 請求送付 → 受諾 → 入金
/// （発注側のNOTICE_CREATED→NOTICE_CONFIRMEDと同じ粒度に揃えたもの。2026-08-18追加）
const RO_STEPS: &[(&str, &str)] = &[
    ("REGISTERED", "受注"),
    ("REPORT_RECEIVED", "勤怠"),
    ("REPORT_SENT", "送付"),
    ("INVOICED", "請求書"),
    ("INVOICE_SENT", "請求送付"),
    ("INVOICE_CONFIRMED", "受諾"),
    ("PAID", "入金"),
];

fn status_done_count(status: &str, steps: &[(&str, &str)]) -> usize {
    for (i, (s, _)) in steps.iter().enumerate() {
        if *s == status {
            return i + 1;
        }
    }
    0
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// テンプレート構造体
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Clone)]
pub struct StepInfo {
    pub label: String,
    pub done: bool,
    pub active: bool,
}

#[derive(Clone)]
pub struct ProgressRow {
    pub entity_name: String,     // パートナー名 or クライアント名
    pub project_id: Option<String>,
    pub project_name: String,
    pub engineer_name: String,
    pub month: String,           // "2026/06"
    pub order_id: String,
    /// 発注進捗行のパートナー契約ID（受注進捗では None）
    pub partner_contract_id: Option<i64>,
    /// 受注進捗行のクライアント契約ID（発注進捗では None）
    pub client_contract_id: Option<i64>,
    pub steps: Vec<StepInfo>,
    pub status: String,          // "complete" | "in_progress" | "not_started"
    pub status_value: String,    // 実際のDBステータス
    pub days_remaining: i64,
    pub action_text: String,     // "あと5日" / "3日超過" / "送付待ち"
}


pub struct FlashMsg {
    pub level: String,
    pub message: String,
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ダッシュボード（GET /）
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// 発注進捗: PurchaseOrder JOIN partner, project, engineer
pub(crate) async fn build_partner_progress(pool: &PgPool, today: NaiveDate) -> Vec<ProgressRow> {
    use crate::infrastructure::repositories::order_repo::list_partner_progress_rows;

    let rows = list_partner_progress_rows(pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("home: fetch_all failed: {:?}", e); vec![] });

    rows.into_iter().map(|r| {
        let done_count = status_done_count(&r.status, PO_STEPS);
        let total = PO_STEPS.len();
        let steps = build_steps(PO_STEPS, done_count);

        // ステータスのフェーズに応じた期限（契約ごとの締め日設定、未設定時は既定値）
        let settings = PoDeadlineSettings {
            order_create_deadline_day: r.order_create_deadline_day.unwrap_or(15),
            order_approve_deadline_days_before: r.order_approve_deadline_days_before.unwrap_or(0),
            report_upload_deadline_days_before: r.report_upload_deadline_days_before.unwrap_or(2),
            invoice_create_deadline_day: r.invoice_create_deadline_day.unwrap_or(1),
            invoice_approve_deadline_day: r.invoice_approve_deadline_day.unwrap_or(10),
        };
        let report_deadline_settings = resolve_report_deadline_settings(
            r.report_deadline_type.as_deref().unwrap_or("RELATIVE"),
            r.report_deadline_value,
            r.report_deadline_holiday_rule.as_deref(),
            settings.report_upload_deadline_days_before,
        );
        let deadline = po_step_deadline(&r.status, r.work_start, r.work_end, &r.payment_condition, &settings, &report_deadline_settings, today);
        let days = (deadline - today).num_days();
        let (status, action_text) = progress_status(done_count, total, days, &r.status);

        let month_str = r.work_start
            .map(|d| format!("{}/{:02}", d.year(), d.month()))
            .unwrap_or_default();

        ProgressRow {
            entity_name: r.partner_name,
            project_id: Some(r.project_id),
            project_name: truncate(&r.project_name, 18),
            engineer_name: truncate(&r.engineer_name, 10),
            month: month_str,
            order_id: r.order_id,
            partner_contract_id: r.partner_contract_id,
            client_contract_id: None,
            steps,
            status,
            status_value: r.status,
            days_remaining: days,
            action_text,
        }
    }).collect()
}

/// 受注進捗: ReceivedOrder JOIN client, engineer
pub(crate) async fn build_client_progress(pool: &PgPool, today: NaiveDate) -> Vec<ProgressRow> {
    use crate::infrastructure::repositories::order_repo::list_client_progress_rows;

    let rows = list_client_progress_rows(pool)
        .await
        .unwrap_or_else(|e| { tracing::warn!("home: fetch_all failed: {:?}", e); vec![] });

    rows.into_iter().map(|r| {
        let done_count = status_done_count(&r.status, RO_STEPS);
        let total = RO_STEPS.len();
        let steps = build_steps(RO_STEPS, done_count);

        // ステータスのフェーズに応じた期限（報告提出期限は案件設定 or 契約の report_deadline_days_before を反映）
        let report_deadline_settings = resolve_report_deadline_settings(
            r.report_deadline_type.as_deref().unwrap_or("RELATIVE"),
            r.report_deadline_value,
            r.report_deadline_holiday_rule.as_deref(),
            r.report_deadline_days_before.unwrap_or(1),
        );
        let deadline = ro_step_deadline(&r.status, r.work_end, &r.payment_condition, &report_deadline_settings, today);
        let days = (deadline - today).num_days();
        let (status, action_text) = progress_status(done_count, total, days, &r.status);

        let month_str = r.target_month
            .map(|d| format!("{}/{:02}", d.year(), d.month()))
            .unwrap_or_default();

        ProgressRow {
            entity_name: r.client_name,
            project_id: r.project_id,
            project_name: truncate(&r.project_name.unwrap_or_default(), 18),
            engineer_name: truncate(&r.engineer_name, 10),
            month: month_str,
            order_id: r.id.to_string(),
            partner_contract_id: None,
            client_contract_id: r.client_contract_id,
            steps,
            status,
            status_value: r.status,
            days_remaining: days,
            action_text,
        }
    }).collect()
}

fn build_steps(step_defs: &[(&str, &str)], done_count: usize) -> Vec<StepInfo> {
    step_defs.iter().enumerate().map(|(i, (_, label))| {
        StepInfo {
            label: label.to_string(),
            done: i < done_count,
            active: i == done_count && i < step_defs.len(),
        }
    }).collect()
}

fn progress_status(done_count: usize, total: usize, days: i64, status: &str) -> (String, String) {
    if done_count >= total {
        ("complete".into(), "完了".into())
    } else if days < 0 {
        ("overdue".into(), format!("{}日超過", -days))
    } else if days <= 3 {
        ("in_progress".into(), format!("あと{}日", days))
    } else {
        // 次のアクションを表示
        let action = match status {
            "DRAFT" => "送付待ち",
            "SENT" => "承諾待ち",
            "ACCEPTED" => "報告待ち",
            "REPORT_RECEIVED" => "請求待ち",
            "NOTICE_CREATED" => "受諾待ち",
            "NOTICE_CONFIRMED" => "支払待ち",
            "REGISTERED" => "勤怠待ち",
            "REPORT_SENT" => "請求書待ち",
            "INVOICED" => "送付待ち",
            "INVOICE_SENT" => "受諾待ち",
            "INVOICE_CONFIRMED" => "入金待ち",
            _ => "進行中",
        };
        ("in_progress".into(), format!("{}\nあと{}日", action, days))
    }
}

/// 発注契約ごとの締め日設定（m_partner_contract、旧EDI_MP OrderBasicInfo相当）
struct PoDeadlineSettings {
    order_create_deadline_day: i32,
    order_approve_deadline_days_before: i32,
    report_upload_deadline_days_before: i32,
    invoice_create_deadline_day: i32,
    invoice_approve_deadline_day: i32,
}

/// 【発注】ステータスのフェーズに応じた期限を算出（契約ごとの締め日設定を反映）
///   DRAFT (発注書作成待ち): 稼働開始月の前月 order_create_deadline_day 日
///   SENT (承諾待ち): 稼働開始月の前月末から order_approve_deadline_days_before 営業日前
///   ACCEPTED (報告待ち): 稼働終了月の月末から report_upload_deadline_days_before 営業日前
///   REPORT_RECEIVED (請求書作成待ち): 稼働終了月の翌月 invoice_create_deadline_day 日
///   NOTICE_CREATED (受諾待ち): 稼働終了月の翌月 invoice_approve_deadline_day 日
///   NOTICE_CONFIRMED以降 (支払待ち): payment_conditionから算出
/// 算出した締め日は最後に roll_to_business_day() で非営業日から繰り上げる
fn po_step_deadline(
    status: &str,
    work_start: Option<NaiveDate>,
    work_end: Option<NaiveDate>,
    payment_condition: &str,
    settings: &PoDeadlineSettings,
    report_deadline_settings: &crate::domain::services::business_day::ReportDeadlineSettings,
    today: NaiveDate,
) -> NaiveDate {
    let start = work_start.unwrap_or(today);
    let end = work_end.unwrap_or(today);

    let deadline = match status {
        "DRAFT" => {
            let (y, m) = sub_months(start.year(), start.month(), 1);
            nth_day_of_month_clip(y, m, settings.order_create_deadline_day)
        }
        "SENT" => {
            let (y, m) = sub_months(start.year(), start.month(), 1);
            let month_end = NaiveDate::from_ymd_opt(y, m, last_day_of_month(y, m)).unwrap_or(start);
            subtract_business_days(month_end, settings.order_approve_deadline_days_before)
        }
        // 案件(m_project)側の稼働報告提出期限設定を受注進捗と共通で使う（2026-08-18、
        // 発注契約側の個別設定を見ていたため受注進捗と期限がズレる不具合の修正）
        "ACCEPTED" => calculate_deadline(end.year(), end.month(), report_deadline_settings)
            .unwrap_or_else(|e| {
                tracing::warn!("発注進捗の提出期限計算に失敗、営業日ベースの簡易計算にフォールバック: {:?}", e);
                subtract_business_days(end, settings.report_upload_deadline_days_before)
            }),
        "REPORT_RECEIVED" => {
            let (y, m) = add_months(end.year(), end.month(), 1);
            nth_day_of_month_clip(y, m, settings.invoice_create_deadline_day)
        }
        "NOTICE_CREATED" => {
            let (y, m) = add_months(end.year(), end.month(), 1);
            nth_day_of_month_clip(y, m, settings.invoice_approve_deadline_day)
        }
        // 支払フェーズ: payment_conditionから算出
        _ => return payment_deadline_from_condition(work_end, payment_condition, today),
    };

    roll_to_business_day(deadline)
}

/// 【受注】ステータスのフェーズに応じた期限を算出
///   REGISTERED (報告待ち): 案件単位の提出期限設定（RELATIVE=月末からN営業日前 / FIXED_DAY=当月N日）。
///     案件側で未設定なら契約の report_deadline_days_before にフォールバック（resolve_report_deadline_settings）
///   REPORT_RECEIVED/REPORT_SENT: 旧EDI_MPにも参照実装がなく、当月末のまま据え置き（スコープ外）
///   INVOICED以降 (入金待ち): payment_conditionから算出
fn ro_step_deadline(
    status: &str,
    work_end: Option<NaiveDate>,
    payment_condition: &str,
    report_deadline_settings: &crate::domain::services::business_day::ReportDeadlineSettings,
    today: NaiveDate,
) -> NaiveDate {
    let end = work_end.unwrap_or(today);

    let deadline = match status {
        "REGISTERED" => calculate_deadline(end.year(), end.month(), report_deadline_settings)
            .unwrap_or_else(|e| {
                tracing::warn!("受注進捗の提出期限計算に失敗、営業日ベースの簡易計算にフォールバック: {:?}", e);
                subtract_business_days(end, report_deadline_settings.value)
            }),
        "REPORT_RECEIVED" | "REPORT_SENT" => end,
        // 支払フェーズ: payment_conditionから算出
        _ => return payment_deadline_from_condition(work_end, payment_condition, today),
    };

    roll_to_business_day(deadline)
}

/// 支払い/入金期限: payment_condition テキストから算出
/// 「翌々月15日払い」→ work_end + 2ヶ月の15日
/// 「翌月末日払い」→ work_end + 1ヶ月の末日
/// デフォルト: 翌々月15日
fn payment_deadline_from_condition(work_end: Option<NaiveDate>, payment_condition: &str, today: NaiveDate) -> NaiveDate {
    let end = work_end.unwrap_or(today);

    // 「X日払い」のパターンを判定（「15日」を先にチェック、「末日」は後で）
    let pay_day = |cond: &str| -> u32 {
        // 「15日払い」「１５日払い」→ 15
        if cond.contains("15日") || cond.contains("１５日") {
            15
        } else if cond.contains("末日払") || cond.contains("末払") {
            0  // 0 = 末日を意味する
        } else {
            15  // デフォルト15日
        }
    };

    if payment_condition.contains("翌々月") {
        let (y, m) = add_months(end.year(), end.month(), 2);
        let day = pay_day(payment_condition);
        let day = if day == 0 { last_day_of_month(y, m) } else { day };
        NaiveDate::from_ymd_opt(y, m, day).unwrap_or(today)
    } else if payment_condition.contains("翌月") {
        let (y, m) = add_months(end.year(), end.month(), 1);
        let day = pay_day(payment_condition);
        let day = if day == 0 { last_day_of_month(y, m) } else { day };
        NaiveDate::from_ymd_opt(y, m, day).unwrap_or(today)
    } else {
        // デフォルト: 翌々月15日
        let (y, m) = add_months(end.year(), end.month(), 2);
        NaiveDate::from_ymd_opt(y, m, 15).unwrap_or(today)
    }
}

fn add_months(year: i32, month: u32, add: u32) -> (i32, u32) {
    let total = month + add;
    if total > 12 {
        (year + (total as i32 - 1) / 12, ((total - 1) % 12) + 1)
    } else {
        (year, total)
    }
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .map(|d| d.pred_opt().map(|p| p.day()).unwrap_or(28))
        .unwrap_or(28)
}

fn sub_months(year: i32, month: u32, sub: u32) -> (i32, u32) {
    let total = month as i32 - sub as i32;
    if total <= 0 {
        let borrow = (-total) / 12 + 1;
        (year - borrow, (total + 12 * borrow) as u32)
    } else {
        (year, total as u32)
    }
}

/// 指定日を month の日数でクリップして NaiveDate を作る（例: 2月31日→2月28日）
fn nth_day_of_month_clip(year: i32, month: u32, day: i32) -> NaiveDate {
    let clipped = (day.max(1) as u32).min(last_day_of_month(year, month));
    NaiveDate::from_ymd_opt(year, month, clipped).unwrap_or_else(|| NaiveDate::from_ymd_opt(year, month, 1).unwrap())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s.to_string()
    }
}

// 行構造体（PurchaseOrderProgressRow, ReceivedOrderProgressRow）は
// infrastructure/repositories/order_repo.rs に移行済み（P2-3, 2026-07-12）

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ステータス変更API
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Deserialize)]
pub struct StatusUpdate {
    pub status: String,
}

/// 発注ステータス変更 POST /orders/{order_id}/status
pub async fn update_purchase_order_status(
    State(pool): State<PgPool>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(order_id): Path<String>,
    Json(body): Json<StatusUpdate>,
) -> impl IntoResponse {
    // ステータスの妥当性チェック
    let valid = PO_STEPS.iter().any(|(s, _)| *s == body.status);
    if !valid {
        return (StatusCode::BAD_REQUEST, Json(StatusResponse {
            ok: false,
            error: Some(format!("無効なステータス: {}", body.status)),
        }));
    }

    let result = crate::infrastructure::repositories::order_repo::update_purchase_order_status(&pool, &order_id, &body.status).await;

    match result {
        Ok(rows) if rows > 0 => {
            (StatusCode::OK, Json(StatusResponse { ok: true, error: None }))
        }
        Ok(_) => {
            (StatusCode::NOT_FOUND, Json(StatusResponse {
                ok: false,
                error: Some("発注書が見つかりません".into()),
            }))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
                ok: false,
                error: Some(e.to_string()),
            }))
        }
    }
}

/// 受注ステータス変更 POST /received-orders/{id}/status
pub async fn update_received_order_status(
    State(pool): State<PgPool>,
    Extension(_auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
    Json(body): Json<StatusUpdate>,
) -> impl IntoResponse {
    let valid = RO_STEPS.iter().any(|(s, _)| *s == body.status);
    if !valid {
        return (StatusCode::BAD_REQUEST, Json(StatusResponse {
            ok: false,
            error: Some(format!("無効なステータス: {}", body.status)),
        }));
    }

    // IDが数値ならid、文字列ならreceived_order_noで検索
    let result = crate::infrastructure::repositories::order_repo::update_received_order_status_by_id_or_no(&pool, &id, &body.status).await;

    match result {
        Ok(rows) if rows > 0 => {
            (StatusCode::OK, Json(StatusResponse { ok: true, error: None }))
        }
        Ok(_) => {
            (StatusCode::NOT_FOUND, Json(StatusResponse {
                ok: false,
                error: Some("受注が見つかりません".into()),
            }))
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(StatusResponse {
                ok: false,
                error: Some(e.to_string()),
            }))
        }
    }
}

// ══════════════════════════════════════════════════════════
// SPA用 JSON API — ダッシュボード
// ══════════════════════════════════════════════════════════

#[derive(Serialize)]
pub struct DashboardApiResponse {
    pub partner_progress: Vec<ProgressRowJson>,
    pub client_progress: Vec<ProgressRowJson>,
    pub partner_list: Vec<String>,
    pub month_list: Vec<String>,
    pub mail_logs: Vec<MailRow>,
    pub mail_confirmed_count: i64,
    pub mail_needs_review_count: i64,  // 人手確認が必要なメール件数
    pub mail_fetch_failed_count: i64,  // 取込に失敗したメール件数
    pub mail_sync_last_processed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize)]
pub struct ProgressRowJson {
    pub entity_name: String,
    pub project_id: Option<String>,
    pub project_name: String,
    pub engineer_name: String,
    pub month: String,
    pub order_id: String,
    pub partner_contract_id: Option<i64>,
    pub client_contract_id: Option<i64>,
    pub steps: Vec<StepInfoJson>,
    pub status: String,
    pub status_value: String,
    pub days_remaining: i64,
    pub action_text: String,
    pub row_status: String,
}

#[derive(Serialize)]
pub struct StepInfoJson {
    pub label: String,
    pub done: bool,
    pub active: bool,
}

impl From<ProgressRow> for ProgressRowJson {
    fn from(r: ProgressRow) -> Self {
        // row_status: complete/overdue/active/pending
        let row_status = if r.status == "complete" {
            "done".to_string()
        } else if r.days_remaining < 0 {
            "overdue".to_string()
        } else {
            "active".to_string()
        };

        Self {
            entity_name: r.entity_name,
            project_id: r.project_id,
            project_name: r.project_name,
            engineer_name: r.engineer_name,
            month: r.month,
            order_id: r.order_id,
            partner_contract_id: r.partner_contract_id,
            client_contract_id: r.client_contract_id,
            steps: r.steps.into_iter().map(|s| StepInfoJson {
                label: s.label,
                done: s.done,
                active: s.active,
            }).collect(),
            status: r.status,
            status_value: r.status_value,
            days_remaining: r.days_remaining,
            action_text: r.action_text,
            row_status,
        }
    }
}

/// GET /api/dashboard — ダッシュボードデータ（JSON）
pub async fn api_dashboard(
    State(pool): State<PgPool>,
) -> Json<DashboardApiResponse> {
    let today = Local::now().date_naive();
    let partner_progress = build_partner_progress(&pool, today).await;
    let client_progress = build_client_progress(&pool, today).await;

    // パートナー名一覧（重複除去）
    let mut partner_set = std::collections::BTreeSet::new();
    let mut month_set = std::collections::BTreeSet::new();
    for r in &partner_progress {
        partner_set.insert(r.entity_name.clone());
        month_set.insert(r.month.clone());
    }

    let partner_rows: Vec<ProgressRowJson> = partner_progress.into_iter().map(Into::into).collect();
    let client_rows: Vec<ProgressRowJson> = client_progress.into_iter().map(Into::into).collect();

    let mail_logs = build_mail_logs(&pool).await;
    let mail_confirmed_count = count_confirmed_mails(&pool).await;
    let mail_needs_review_count = super::mail::count_needs_review_mails(&pool).await;
    let mail_fetch_failed_count = super::mail::count_fetch_failed_mails(&pool).await;
    let mail_sync_last_processed_at = mail_sync_last_processed_at(&pool).await;

    Json(DashboardApiResponse {
        partner_progress: partner_rows,
        client_progress: client_rows,
        partner_list: partner_set.into_iter().collect(),
        month_list: month_set.into_iter().rev().collect(),
        mail_logs,
        mail_confirmed_count,
        mail_needs_review_count,
        mail_fetch_failed_count,
        mail_sync_last_processed_at,
    })
}

#[derive(Deserialize)]
pub struct ProjectDashboardQuery {
    pub month: Option<String>,
}

/// GET /api/dashboard/projects?month=2026-07-01 — プロジェクト別ダッシュボードデータ（JSON）
pub async fn api_project_dashboard(
    State(pool): State<PgPool>,
    axum::extract::Query(q): axum::extract::Query<ProjectDashboardQuery>,
) -> Result<Json<Vec<crate::domain::services::project_dashboard::ProjectSummary>>, crate::presentation::api_response::AppError> {
    let today = chrono::Utc::now().date_naive();
    let month_str = q.month.unwrap_or_else(|| format!("{}-{:02}-01", today.year(), today.month()));
    let target_month = NaiveDate::parse_from_str(&month_str, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", month_str), "%Y-%m-%d"))
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap());

    let summaries = crate::domain::services::project_dashboard::list_project_summaries(&pool, target_month).await?;
    Ok(Json(summaries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::business_day::roll_to_business_day;

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    // ── add_months / sub_months ──

    #[test]
    fn add_months_within_same_year() {
        assert_eq!(add_months(2026, 3, 2), (2026, 5));
    }

    #[test]
    fn add_months_rolls_over_year_boundary() {
        assert_eq!(add_months(2026, 11, 2), (2027, 1));
        assert_eq!(add_months(2026, 12, 1), (2027, 1));
    }

    #[test]
    fn add_months_rolls_over_multiple_years() {
        assert_eq!(add_months(2026, 6, 20), (2028, 2));
    }

    #[test]
    fn sub_months_within_same_year() {
        assert_eq!(sub_months(2026, 5, 2), (2026, 3));
    }

    #[test]
    fn sub_months_borrows_across_year_boundary() {
        assert_eq!(sub_months(2026, 1, 1), (2025, 12));
        assert_eq!(sub_months(2026, 2, 3), (2025, 11));
    }

    #[test]
    fn sub_months_borrows_across_multiple_years() {
        assert_eq!(sub_months(2026, 1, 14), (2024, 11));
    }

    // ── last_day_of_month / nth_day_of_month_clip ──

    #[test]
    fn last_day_of_month_regular_months() {
        assert_eq!(last_day_of_month(2026, 1), 31);
        assert_eq!(last_day_of_month(2026, 4), 30);
        assert_eq!(last_day_of_month(2026, 12), 31);
    }

    #[test]
    fn last_day_of_month_handles_leap_year_february() {
        assert_eq!(last_day_of_month(2024, 2), 29); // 閏年
        assert_eq!(last_day_of_month(2026, 2), 28); // 平年
    }

    #[test]
    fn nth_day_of_month_clip_clips_to_month_end() {
        // 2月31日 → 平年は28日、閏年は29日にクリップ
        assert_eq!(nth_day_of_month_clip(2026, 2, 31), ymd(2026, 2, 28));
        assert_eq!(nth_day_of_month_clip(2024, 2, 31), ymd(2024, 2, 29));
    }

    #[test]
    fn nth_day_of_month_clip_floors_to_first_day() {
        assert_eq!(nth_day_of_month_clip(2026, 5, 0), ymd(2026, 5, 1));
    }

    #[test]
    fn nth_day_of_month_clip_normal_day() {
        assert_eq!(nth_day_of_month_clip(2026, 5, 15), ymd(2026, 5, 15));
    }

    // ── status_done_count / build_steps ──

    #[test]
    fn status_done_count_matches_position_in_steps() {
        assert_eq!(status_done_count("DRAFT", PO_STEPS), 1);
        assert_eq!(status_done_count("PAID", PO_STEPS), PO_STEPS.len());
    }

    #[test]
    fn status_done_count_unknown_status_is_zero() {
        assert_eq!(status_done_count("UNKNOWN", PO_STEPS), 0);
    }

    #[test]
    fn build_steps_marks_done_and_active_correctly() {
        let steps = build_steps(PO_STEPS, 2);
        assert!(steps[0].done && !steps[0].active);
        assert!(steps[1].done && !steps[1].active);
        assert!(!steps[2].done && steps[2].active);
        assert!(!steps[3].done && !steps[3].active);
    }

    // ── progress_status ──

    #[test]
    fn progress_status_complete_when_all_steps_done() {
        let (status, text) = progress_status(PO_STEPS.len(), PO_STEPS.len(), 5, "PAID");
        assert_eq!(status, "complete");
        assert_eq!(text, "完了");
    }

    #[test]
    fn progress_status_overdue_when_days_negative() {
        let (status, text) = progress_status(1, PO_STEPS.len(), -3, "DRAFT");
        assert_eq!(status, "overdue");
        assert_eq!(text, "3日超過");
    }

    #[test]
    fn progress_status_in_progress_within_three_days() {
        let (status, text) = progress_status(1, PO_STEPS.len(), 2, "DRAFT");
        assert_eq!(status, "in_progress");
        assert_eq!(text, "あと2日");
    }

    #[test]
    fn progress_status_shows_action_text_for_known_status() {
        let (status, text) = progress_status(1, PO_STEPS.len(), 10, "SENT");
        assert_eq!(status, "in_progress");
        assert_eq!(text, "承諾待ち\nあと10日");
    }

    #[test]
    fn progress_status_falls_back_for_unknown_status() {
        let (_, text) = progress_status(1, PO_STEPS.len(), 10, "UNKNOWN");
        assert_eq!(text, "進行中\nあと10日");
    }

    // ── truncate ──

    #[test]
    fn truncate_leaves_short_ascii_untouched() {
        assert_eq!(truncate("hello", 18), "hello");
    }

    #[test]
    fn truncate_shortens_long_ascii_with_ellipsis() {
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn truncate_counts_multibyte_chars_not_bytes() {
        // 日本語10文字（3バイト/文字）を4文字+省略記号に切り詰め
        let s = "あいうえおかきくけこ";
        assert_eq!(truncate(s, 5), "あいうえ…");
    }

    // ── payment_deadline_from_condition ──

    #[test]
    fn payment_deadline_translates_after_next_month_15th() {
        let end = ymd(2026, 5, 20);
        let deadline = payment_deadline_from_condition(Some(end), "翌々月15日払い", end);
        assert_eq!(deadline, ymd(2026, 7, 15));
    }

    #[test]
    fn payment_deadline_translates_next_month_end() {
        let end = ymd(2026, 5, 20);
        let deadline = payment_deadline_from_condition(Some(end), "翌月末日払い", end);
        assert_eq!(deadline, ymd(2026, 6, 30));
    }

    #[test]
    fn payment_deadline_defaults_to_next_next_month_15th_when_unrecognized() {
        let end = ymd(2026, 5, 20);
        let deadline = payment_deadline_from_condition(Some(end), "", end);
        assert_eq!(deadline, ymd(2026, 7, 15));
    }

    // ── po_step_deadline / ro_step_deadline（支払フェーズはbusiness_day未適用のため決定的）──

    #[test]
    fn po_step_deadline_payment_phase_uses_payment_condition_without_business_day_roll() {
        let settings = PoDeadlineSettings {
            order_create_deadline_day: 15,
            order_approve_deadline_days_before: 0,
            report_upload_deadline_days_before: 2,
            invoice_create_deadline_day: 1,
            invoice_approve_deadline_day: 10,
        };
        let work_end = Some(ymd(2026, 5, 31));
        let today = ymd(2026, 6, 1);
        let report_deadline_settings = resolve_report_deadline_settings("RELATIVE", None, None, 2);
        let deadline = po_step_deadline("NOTICE_CONFIRMED", None, work_end, "翌々月15日払い", &settings, &report_deadline_settings, today);
        assert_eq!(deadline, ymd(2026, 7, 15));
    }

    #[test]
    fn po_step_deadline_draft_phase_rolls_to_business_day() {
        let settings = PoDeadlineSettings {
            order_create_deadline_day: 15,
            order_approve_deadline_days_before: 0,
            report_upload_deadline_days_before: 2,
            invoice_create_deadline_day: 1,
            invoice_approve_deadline_day: 10,
        };
        let work_start = Some(ymd(2026, 6, 1));
        let today = ymd(2026, 5, 1);
        let report_deadline_settings = resolve_report_deadline_settings("RELATIVE", None, None, 2);
        let deadline = po_step_deadline("DRAFT", work_start, None, "", &settings, &report_deadline_settings, today);
        // 前月(5月)の15日が起点。非営業日なら繰り上げられる
        let expected = roll_to_business_day(ymd(2026, 5, 15));
        assert_eq!(deadline, expected);
    }

    #[test]
    fn ro_step_deadline_payment_phase_uses_payment_condition() {
        let work_end = Some(ymd(2026, 5, 31));
        let today = ymd(2026, 6, 1);
        let settings = crate::domain::services::business_day::ReportDeadlineSettings {
            deadline_type: crate::domain::services::business_day::ReportDeadlineType::from_str("RELATIVE"),
            value: 1,
            holiday_rule: None,
        };
        let deadline = ro_step_deadline("PAID", work_end, "翌月末日払い", &settings, today);
        assert_eq!(deadline, ymd(2026, 6, 30));
    }

    #[test]
    fn ro_step_deadline_registered_phase_uses_project_fixed_day_settings() {
        // 案件側でFIXED_DAY(当月20日、翌営業日ルール)を設定している場合、
        // 稼働終了月(2026年7月)の20日は海の日で非営業日のため7/21に繰り下がる
        let work_end = Some(ymd(2026, 7, 31));
        let today = ymd(2026, 7, 1);
        let settings = crate::domain::services::business_day::ReportDeadlineSettings {
            deadline_type: crate::domain::services::business_day::ReportDeadlineType::from_str("FIXED_DAY"),
            value: 20,
            holiday_rule: crate::domain::services::business_day::ReportDeadlineHolidayRule::from_str("NEXT_BUSINESS_DAY"),
        };
        let deadline = ro_step_deadline("REGISTERED", work_end, "", &settings, today);
        assert_eq!(deadline, ymd(2026, 7, 21));
    }
}
