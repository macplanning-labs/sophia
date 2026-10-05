use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
    Form,
};
use chrono::NaiveDate;
use sqlx::PgPool;
use crate::domain::services::settlement_dashboard::{self, SettlementFilter};
use super::IssueForm;

/// POST /monthly-settlement/issue-notices — 支払通知書一括発行（パートナー集約）
pub async fn issue_notices(
    State(pool): State<PgPool>,
    Form(form): Form<IssueForm>,
) -> impl IntoResponse {
    let target_month = NaiveDate::parse_from_str(&form.target_month, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&format!("{}-01", form.target_month), "%Y-%m-%d"))
        .unwrap_or_else(|_| chrono::Utc::now().date_naive());

    let selected: Vec<i64> = form.selected_ids
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if selected.is_empty() {
        return Redirect::to("/monthly-settlement").into_response();
    }

    // 選択された行のデータを取得
    let filter = SettlementFilter::default();
    let rows = match settlement_dashboard::list_settlement_rows(&pool, target_month, &filter).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!("支払通知書発行: settlement_dashboard エラー: {:?}", e);
            return Redirect::to("/monthly-settlement").into_response();
        }
    };
    let (view_rows, _) = settlement_dashboard::calculate_preview(rows);

    // 選択されたpartner_contract_idのみフィルタ
    let selected_rows: Vec<_> = view_rows.iter()
        .filter(|vr| vr.row.partner_contract_id.map_or(false, |id| selected.contains(&id)))
        .collect();

    match settlement_dashboard::create_notices_by_partner(&pool, &selected_rows.iter().map(|r| (*r).clone()).collect::<Vec<_>>(), target_month).await {
        Ok(_results) => {
            Redirect::to(&format!("/monthly-settlement?month={}", form.target_month)).into_response()
        }
        Err(e) => {
            tracing::error!("支払通知書一括発行エラー: {}", e);
            Redirect::to("/monthly-settlement").into_response()
        }
    }
}

/// POST /monthly-settlement/issue-invoices — 請求書一括発行
/// ※ 旧SSR用スタブ。SPAではapi_issue_invoices()を使用。
/// ※ 実ロジックは未接続。個別発行は invoices.rs 請求書詳細画面から可能。
pub async fn issue_invoices(
    State(_pool): State<PgPool>,
    Form(form): Form<IssueForm>,
) -> impl IntoResponse {
    tracing::warn!("⚠️ issue_invoices() が呼ばれましたが、一括発行ロジックは未実装です (target_month={})", form.target_month);
    Redirect::to(&format!("/monthly-settlement?month={}", form.target_month))
}

/// POST /monthly-settlement/send-notices — メール一括送付
/// ※ 旧SSR用スタブ。実ロジックは未接続。
/// ※ 個別送信は notices.rs 支払通知詳細画面から可能。
pub async fn send_notices(
    State(_pool): State<PgPool>,
    Form(form): Form<IssueForm>,
) -> impl IntoResponse {
    tracing::warn!("⚠️ send_notices() が呼ばれましたが、一括送付ロジックは未実装です (target_month={})", form.target_month);
    Redirect::to(&format!("/monthly-settlement?month={}", form.target_month))
}
