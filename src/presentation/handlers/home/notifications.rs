/// presentation/handlers/home/notifications.rs — 期限横断通知API
///
/// ダッシュボードの ProgressRow 期限計算を再利用し、
/// overdue / due_soon(≤3日) を横断一覧する(WS5)。

use axum::{extract::State, Json};
use chrono::Local;
use serde::Serialize;
use sqlx::PgPool;

use super::dashboard::{build_client_progress, build_partner_progress, ProgressRow};

/// due soon 閾値(日)。dashboard::progress_status の `days <= 3` と統一。
const DUE_SOON_DAYS: i64 = 3;

#[derive(Debug, Clone, Serialize)]
pub struct NotificationItem {
    pub id: String,
    pub category: String,
    pub category_label: String,
    pub title: String,
    pub message: String,
    pub severity: String,
    pub days_remaining: i64,
    pub href: String,
    pub project_id: Option<String>,
    pub project_name: String,
}

#[derive(Debug, Serialize)]
pub struct NotificationsResponse {
    pub count: usize,
    pub overdue_count: usize,
    pub items: Vec<NotificationItem>,
}

/// GET /api/notifications — 要対応期限の横断一覧
pub async fn api_notifications(State(pool): State<PgPool>) -> Json<NotificationsResponse> {
    let today = Local::now().date_naive();
    let items = collect_notification_items(&pool, today).await;
    let overdue_count = items.iter().filter(|i| i.severity == "overdue").count();
    Json(NotificationsResponse {
        count: items.len(),
        overdue_count,
        items,
    })
}

pub(crate) async fn collect_notification_items(
    pool: &PgPool,
    today: chrono::NaiveDate,
) -> Vec<NotificationItem> {
    let partner = build_partner_progress(pool, today).await;
    let client = build_client_progress(pool, today).await;

    let mut items: Vec<NotificationItem> = partner
        .into_iter()
        .filter_map(|r| map_po_row(r))
        .chain(client.into_iter().filter_map(|r| map_ro_row(r)))
        .collect();

    items.sort_by(|a, b| {
        let sev = |s: &str| if s == "overdue" { 0 } else { 1 };
        sev(&a.severity)
            .cmp(&sev(&b.severity))
            .then(a.days_remaining.cmp(&b.days_remaining))
            .then(a.category.cmp(&b.category))
    });

    items
}

fn severity_for(days: i64, status: &str) -> Option<&'static str> {
    if status == "complete" {
        return None;
    }
    if days < 0 {
        Some("overdue")
    } else if days <= DUE_SOON_DAYS {
        Some("due_soon")
    } else {
        None
    }
}

fn map_po_row(r: ProgressRow) -> Option<NotificationItem> {
    let severity = severity_for(r.days_remaining, &r.status)?;
    let (category, category_label) = match r.status_value.as_str() {
        "DRAFT" | "SENT" => ("purchase_order", "発注書"),
        "ACCEPTED" => ("timesheet", "稼働報告"),
        "REPORT_RECEIVED" | "NOTICE_CREATED" => ("payment_notice", "支払通知"),
        _ => return None,
    };
    Some(NotificationItem {
        id: format!("{category}:po:{}", r.order_id),
        category: category.into(),
        category_label: category_label.into(),
        title: format!("{} / {}", r.project_name, r.engineer_name),
        message: r.action_text.replace('\n', " "),
        severity: severity.into(),
        days_remaining: r.days_remaining,
        href: format!("/orders/{}", r.order_id),
        project_id: r.project_id,
        project_name: r.project_name,
    })
}

fn map_ro_row(r: ProgressRow) -> Option<NotificationItem> {
    let severity = severity_for(r.days_remaining, &r.status)?;
    let (category, category_label) = match r.status_value.as_str() {
        "REGISTERED" => ("timesheet", "稼働報告"),
        "REPORT_SENT" | "INVOICED" => ("invoice", "請求書"),
        _ => return None,
    };
    Some(NotificationItem {
        id: format!("{category}:ro:{}", r.order_id),
        category: category.into(),
        category_label: category_label.into(),
        title: format!("{} / {}", r.project_name, r.engineer_name),
        message: r.action_text.replace('\n', " "),
        severity: severity.into(),
        days_remaining: r.days_remaining,
        href: format!("/received-orders/{}", r.order_id),
        project_id: r.project_id,
        project_name: r.project_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_overdue_and_due_soon() {
        assert_eq!(severity_for(-1, "in_progress"), Some("overdue"));
        assert_eq!(severity_for(0, "in_progress"), Some("due_soon"));
        assert_eq!(severity_for(3, "in_progress"), Some("due_soon"));
        assert_eq!(severity_for(4, "in_progress"), None);
        assert_eq!(severity_for(0, "complete"), None);
    }
}
