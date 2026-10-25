use axum::{
    extract::{Query, State},
    Json, response::IntoResponse,
};
use crate::infrastructure::db_tx::LogErr;
use chrono::{NaiveDate, Datelike};
use sqlx::PgPool;
use crate::domain::authz::stamp::NO_OWNER_MESSAGE;
use crate::domain::services::issue_error::IssueError;
use crate::domain::services::settlement_dashboard::{
    self, SettlementFilter,
};
use crate::presentation::api_response::AppError;
use super::{
    ClientOption, PartnerOption, ProjectOption, DashboardFilter, SettlementApiResponse,
    FiltersApiResponse, MonthOption, ApiIssueForm, generate_month_list,
};

/// GET /api/settlement?month=2026-05-01&client_id=&partner_id=&status=
pub async fn api_index(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Query(filter): Query<DashboardFilter>,
) -> Result<Json<SettlementApiResponse>, AppError> {
    let today = chrono::Utc::now().date_naive();
    let target_month_str = filter.month.clone().unwrap_or_else(|| {
        format!("{}-{:02}-01", today.year(), today.month())
    });
    let target_month = NaiveDate::parse_from_str(&target_month_str, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", target_month_str), "%Y-%m-%d"))
        .unwrap_or_else(|_| today.with_day(1).unwrap_or(today));

    // partner_id の条件は、SQL ではなく、支払側を伏せた後の行に掛ける(見えないパートナーの所属を、条件で逆算させない)
    let sfilter = SettlementFilter {
        target_month: Some(target_month_str),
        client_id: filter.client_id,
        partner_id: None,
        project_id: filter.project_id.clone(),
        status: filter.status.clone(),
    };

    let rows = settlement_dashboard::list_settlement_rows(&pool, target_month, &sfilter).await?;
    // 見える範囲の行だけにする(集計も、見える行だけで行う。入口の権限表に頼らない。DEMO-000254)
    let (rows, visibility) = retain_visible_rows(&pool, &auth_user, authz.as_ref().map(|e| &e.0), rows).await?;
    let (rows, visibility): (Vec<_>, Vec<_>) = rows
        .into_iter()
        .zip(visibility)
        .filter(|(r, _)| filter.partner_id.as_deref().filter(|p| !p.is_empty()).is_none_or(|p| r.partner_id.as_deref() == Some(p)))
        .unzip();
    let (mut view_rows, mut summary) = settlement_dashboard::calculate_preview(rows);
    settlement_dashboard::finish_redacted_preview(&mut view_rows, &mut summary, &visibility);

    // ステータスフィルタ
    if let Some(ref status) = filter.status {
        view_rows = view_rows.into_iter().filter(|vr| {
            match status.as_str() {
                "pending" => !vr.row.invoice_issued.unwrap_or(false) || !vr.row.notice_issued.unwrap_or(false),
                "invoice_issued" => vr.row.invoice_issued.unwrap_or(false),
                "notice_issued" => vr.row.notice_issued.unwrap_or(false),
                "complete" => vr.row.invoice_issued.unwrap_or(false) && vr.row.notice_issued.unwrap_or(false),
                _ => true,
            }
        }).collect();
    }

    let current_month = target_month.format("%Y年%m月").to_string();

    Ok(Json(SettlementApiResponse {
        rows: view_rows,
        summary,
        current_month,
    }))
}

/// GET /api/settlement/filters — フィルタ用マスタデータ
pub async fn api_filters(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
) -> axum::response::Response {
    // 担当スタンプを持たない操作(一覧・選択肢・新規作成)。権限を持つ人だけが扱える。入口の権限表に頼らない(DEMO-000254)
    if !auth_user.can_access_unstamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::BillingInvoiceView) {
        return crate::presentation::http_util::forbidden();
    }
    axum::response::IntoResponse::into_response(api_filters_inner(State(pool)).await)
}

async fn api_filters_inner(
    State(pool): State<PgPool>,
) -> Json<FiltersApiResponse> {
    let today = chrono::Utc::now().date_naive();

    let clients: Vec<ClientOption> = crate::infrastructure::repositories::client_repo::list_id_name_options(&pool)
        .await
        .log_err().unwrap_or_default()
        .into_iter()
        .map(|(id, name)| ClientOption { id, name })
        .collect();

    let partners: Vec<PartnerOption> = crate::infrastructure::repositories::partner_repo::list_id_name_options(&pool)
        .await
        .log_err().unwrap_or_default()
        .into_iter()
        .map(|(partner_id, name)| PartnerOption { partner_id, name })
        .collect();

    let projects: Vec<ProjectOption> = crate::infrastructure::repositories::project_repo::list_id_name_options(&pool)
        .await
        .log_err().unwrap_or_default()
        .into_iter()
        .map(|(project_id, name)| ProjectOption { project_id, name })
        .collect();

    let months = generate_month_list(today, 6);
    let available_months: Vec<MonthOption> = months.into_iter()
        .map(|(v, l)| MonthOption { value: v, label: l })
        .collect();

    Json(FiltersApiResponse {
        available_months,
        clients,
        partners,
        projects,
    })
}

/// 精算の行のうち、見える範囲のものだけを残す。行は、請求側(受注契約)を請求書の閲覧権限で、
/// 支払側(発注契約)を支払通知の閲覧権限で判定し、どちらかが見えれば残す。
/// 片側しか見えない行は、見えない側の項目を伏せる(DEMO-000260)。返す `visibility` は、行と同じ順の `(請求側, 支払側)`。
async fn retain_visible_rows(
    pool: &PgPool,
    auth_user: &crate::presentation::middleware::role::AuthUser,
    authz: Option<&crate::domain::authz::Authz>,
    rows: Vec<settlement_dashboard::SettlementRow>,
) -> Result<(Vec<settlement_dashboard::SettlementRow>, Vec<(bool, bool)>), AppError> {
    use crate::domain::authz::Permission;
    use crate::infrastructure::repositories::stamp_repo;
    let invoice_filter = auth_user.list_filter(authz, Permission::BillingInvoiceView);
    let notice_filter = auth_user.list_filter(authz, Permission::BillingPaymentView);
    let cc_ids: Vec<i64> = rows.iter().map(|r| r.client_contract_id).collect();
    let pc_ids: Vec<i64> = rows.iter().filter_map(|r| r.partner_contract_id).collect();
    let cc = stamp_repo::of_client_contracts(pool, &cc_ids).await?;
    let pc = stamp_repo::of_partner_contracts(pool, &pc_ids).await?;
    let mut out_rows = Vec::new();
    let mut visibility = Vec::new();
    for mut r in rows {
        let inv = invoice_filter.allows_all() || cc.get(&r.client_contract_id).is_some_and(|s| invoice_filter.matches(s));
        // 支払側が無い行(社員。発注契約なし)は、伏せる項目が無いので「見える」扱い。ただし行を残す理由にはしない
        let pay_exists = r.partner_contract_id.is_some();
        let pay = notice_filter.allows_all() || r.partner_contract_id.and_then(|id| pc.get(&id)).is_some_and(|s| notice_filter.matches(s));
        if !inv && !(pay_exists && pay) {
            continue;
        }
        let pay_shown = !pay_exists || pay;
        settlement_dashboard::mask_hidden_sides(&mut r, inv, pay_shown);
        out_rows.push(r);
        visibility.push((inv, pay_shown));
    }
    Ok((out_rows, visibility))
}

/// 一括発行APIのエラー。担当が決まらないときだけ 400(本文は従来の発行結果と同じ `success`/`message` の形)、
/// それ以外は共通の `AppError`(500)。
pub enum IssueApiError {
    /// 選んだ対象に、権限または担当範囲の外のものがある(何も発行しない)。
    Forbidden,
    /// 顧客・案件に担当が無く、請求書・支払通知書を作れない(全体を取り消している)。
    NoOwner,
    /// その他の内部エラー。
    App(AppError),
}

impl<E: Into<anyhow::Error>> From<E> for IssueApiError {
    fn from(e: E) -> Self {
        IssueApiError::App(AppError::from(e))
    }
}

impl IntoResponse for IssueApiError {
    fn into_response(self) -> axum::response::Response {
        match self {
            IssueApiError::Forbidden => crate::presentation::http_util::forbidden(),
            IssueApiError::NoOwner => {
                tracing::warn!("一括発行を拒否しました(担当が決まらない)");
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "success": false, "message": NO_OWNER_MESSAGE, "error": NO_OWNER_MESSAGE })),
                )
                    .into_response()
            }
            IssueApiError::App(e) => e.into_response(),
        }
    }
}

/// POST /api/settlement/issue-invoices — 請求書一括発行（JSON版）
pub async fn api_issue_invoices(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Json(form): Json<ApiIssueForm>,
) -> Result<Json<serde_json::Value>, IssueApiError> {
    // 選んだ受注契約のすべてが、発行の権限と担当範囲の中であること(1件でも外なら、何も発行しない)。入口の権限表に頼らない(DEMO-000254)
    let stamps = crate::infrastructure::repositories::stamp_repo::of_client_contracts(&pool, &form.selected_ids).await?;
    for id in &form.selected_ids {
        let stamp = stamps.get(id).cloned().unwrap_or_default();
        if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::BillingInvoiceIssue, &stamp) {
            return Err(IssueApiError::Forbidden);
        }
    }
    let target_month = NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", form.target_month), "%Y-%m-%d"))
        .unwrap_or_else(|_| chrono::Utc::now().date_naive());

    if form.selected_ids.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "対象が選択されていません",
        })));
    }

    let filter = SettlementFilter::default();
    let rows = settlement_dashboard::list_settlement_rows(&pool, target_month, &filter).await?;
    let (view_rows, _) = settlement_dashboard::calculate_preview(rows);

    let selected_rows: Vec<_> = view_rows.iter()
        .filter(|vr| form.selected_ids.contains(&vr.row.client_contract_id))
        .cloned()
        .collect();

    let edi_excluded_count = selected_rows.iter()
        .filter(|vr| vr.row.client_invoice_by_client)
        .count();

    match settlement_dashboard::create_invoices_by_client(&pool, &selected_rows, target_month).await {
        Ok(results) => {
            if results.is_empty() {
                let blockers = settlement_dashboard::diagnose_invoice_issue_blockers(
                    &pool,
                    &selected_rows,
                    target_month,
                )
                .await;
                let message = if blockers.is_empty() {
                    if selected_rows.is_empty() {
                        "対象行が見つかりませんでした（選択が無効になっている可能性があります）。画面を更新して再度お試しください。".to_string()
                    } else {
                        "請求書を発行できませんでした。管理者に連絡してください。".to_string()
                    }
                } else {
                    use crate::domain::models::actionable_error::format_action_blockers_message;
                    format_action_blockers_message(&blockers, "請求書を発行できませんでした。条件を確認してください。")
                };
                return Ok(Json(serde_json::json!({
                    "success": false,
                    "message": message,
                    "blockers": blockers,
                    "issued_ids": form.selected_ids,
                })));
            }

            let message = if edi_excluded_count > 0 {
                format!(
                    "{}件の請求書を発行しました（EDI連携クライアントの{}件は対象外としてスキップしました）",
                    results.len(), edi_excluded_count
                )
            } else {
                format!("{}件の請求書を発行しました", results.len())
            };
            Ok(Json(serde_json::json!({
                "success": true,
                "message": message,
                "issued_ids": form.selected_ids,
                "created_invoices": results,
            })))
        }
        // 担当が決まらない: 400 と案内(全体を取り消している)
        Err(IssueError::NoOwner) => Err(IssueApiError::NoOwner),
        Err(e) => {
            Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("発行エラー: {}", e),
            })))
        }
    }
}

/// POST /api/settlement/issue-notices — 支払通知書一括発行（JSON版）
pub async fn api_issue_notices(
    State(pool): State<PgPool>,
    axum::Extension(auth_user): axum::Extension<crate::presentation::middleware::role::AuthUser>,
    authz: Option<axum::Extension<crate::domain::authz::Authz>>,
    Json(form): Json<ApiIssueForm>,
) -> Result<Json<serde_json::Value>, IssueApiError> {
    // 選んだ発注契約のすべてが、発行の権限と担当範囲の中であること(1件でも外なら、何も発行しない)。入口の権限表に頼らない(DEMO-000254)
    let stamps = crate::infrastructure::repositories::stamp_repo::of_partner_contracts(&pool, &form.selected_ids).await?;
    for id in &form.selected_ids {
        let stamp = stamps.get(id).cloned().unwrap_or_default();
        if !auth_user.can_access_stamped(authz.as_ref().map(|e| &e.0), crate::domain::authz::Permission::BillingPaymentIssue, &stamp) {
            return Err(IssueApiError::Forbidden);
        }
    }
    let target_month = NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", form.target_month), "%Y-%m-%d"))
        .unwrap_or_else(|_| chrono::Utc::now().date_naive());

    if form.selected_ids.is_empty() {
        return Ok(Json(serde_json::json!({
            "success": false,
            "message": "対象が選択されていません",
        })));
    }

    let filter = SettlementFilter::default();
    let rows = settlement_dashboard::list_settlement_rows(&pool, target_month, &filter).await?;
    let (view_rows, _) = settlement_dashboard::calculate_preview(rows);

    let selected_rows: Vec<_> = view_rows.iter()
        .filter(|vr| {
            vr.row
                .partner_contract_id
                .map_or(false, |id| form.selected_ids.contains(&id))
        })
        .cloned()
        .collect();

    match settlement_dashboard::create_notices_by_partner(&pool, &selected_rows, target_month).await {
        Ok(results) => {
            if results.is_empty() {
                let blockers = settlement_dashboard::diagnose_notice_issue_blockers(
                    &pool,
                    &selected_rows,
                    target_month,
                )
                .await;
                let message = if blockers.is_empty() {
                    if selected_rows.is_empty() {
                        "対象行が見つかりませんでした（選択が無効になっている可能性があります）。画面を更新して再度お試しください。".to_string()
                    } else {
                        "支払通知書を発行できませんでした。管理者に連絡してください。".to_string()
                    }
                } else {
                    settlement_dashboard::format_notice_issue_blockers_message(&blockers)
                };
                return Ok(Json(serde_json::json!({
                    "success": false,
                    "message": message,
                    "blockers": blockers,
                    "issued_ids": form.selected_ids,
                })));
            }
            Ok(Json(serde_json::json!({
                "success": true,
                "message": format!("{}件の支払通知書を発行しました", results.len()),
                "issued_ids": form.selected_ids,
                "created_notices": results,
            })))
        }
        // 担当が決まらない: 400 と案内(全体を取り消している)
        Err(IssueError::NoOwner) => Err(IssueApiError::NoOwner),
        Err(e) => {
            Ok(Json(serde_json::json!({
                "success": false,
                "message": format!("発行エラー: {}", e),
            })))
        }
    }
}

