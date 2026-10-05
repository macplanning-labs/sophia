/// infrastructure/repositories/reminder_repo.rs — リマインドメール用リポジトリ

use anyhow::Result;
use chrono::NaiveDate;
use sqlx::PgPool;

/// 注文書承諾リマインド対象（SENT 3日超）
#[derive(Debug, sqlx::FromRow, Clone)]
pub struct PendingOrderReminder {
    pub order_id: String,
    pub partner_name: String,
    pub partner_email: String,
    pub uuid: uuid::Uuid,
    pub days_pending: i32,
}

/// 請求書承諾リマインド対象
#[derive(Debug, sqlx::FromRow, Clone)]
pub struct PendingInvoiceReminder {
    pub notice_id: String,
    pub partner_name: String,
    pub partner_email: String,
    pub uuid: uuid::Uuid,
    pub target_month: NaiveDate,
    pub total: i32,
}

/// 期限超過注文書
#[derive(Debug, sqlx::FromRow, Clone)]
pub struct OverdueOrder {
    pub order_id: String,
    pub partner_name: String,
    pub days_pending: i32,
    pub work_start: Option<NaiveDate>,
    pub work_end: Option<NaiveDate>,
}

/// 支払期限チェック
#[derive(Debug, sqlx::FromRow, Clone)]
pub struct PaymentCheckReminder {
    pub notice_id: String,
    pub partner_name: String,
    pub is_today: bool,
}

/// 注文書承諾リマインド対象を取得
pub async fn list_pending_order_reminders(pool: &PgPool) -> Result<Vec<PendingOrderReminder>> {
    let rows = sqlx::query_as::<_, PendingOrderReminder>(
        r#"SELECT po.order_id,
                  COALESCE(p.name, '') as partner_name,
                  COALESCE(p.email, '') as partner_email,
                  po.uuid,
                  EXTRACT(DAY FROM NOW() - po.token_issued_at)::int as days_pending
           FROM t_purchase_order po
           JOIN m_partner p ON p.partner_id = po.partner_id
           WHERE po.status = 'SENT'
             AND po.token_issued_at IS NOT NULL
             AND po.token_issued_at < NOW() - INTERVAL '3 days'
             AND p.email IS NOT NULL AND p.email != ''
           ORDER BY po.token_issued_at"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 請求書承諾リマインド対象を取得
pub async fn list_pending_invoice_reminders(pool: &PgPool) -> Result<Vec<PendingInvoiceReminder>> {
    let rows = sqlx::query_as::<_, PendingInvoiceReminder>(
        r#"SELECT pn.notice_id,
                  COALESCE(p.name, '') as partner_name,
                  COALESCE(p.email, '') as partner_email,
                  pn.uuid,
                  pn.target_month,
                  pn.total
           FROM t_payment_notice pn
           JOIN t_purchase_order po ON po.id = pn.purchase_order_pk
           JOIN m_partner p ON p.partner_id = po.partner_id
           WHERE pn.partner_accepted_at IS NULL
             AND pn.created_at < NOW() - INTERVAL '3 days'
             AND p.email IS NOT NULL AND p.email != ''
           ORDER BY pn.created_at"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 期限超過注文書を取得
pub async fn list_overdue_orders(pool: &PgPool) -> Result<Vec<OverdueOrder>> {
    let rows = sqlx::query_as::<_, OverdueOrder>(
        r#"SELECT po.order_id,
                  COALESCE(p.name, '') as partner_name,
                  EXTRACT(DAY FROM NOW() - po.token_issued_at)::int as days_pending,
                  po.work_start, po.work_end
           FROM t_purchase_order po
           JOIN m_partner p ON p.partner_id = po.partner_id
           WHERE po.status = 'SENT'
             AND po.token_issued_at IS NOT NULL
             AND po.token_issued_at < NOW() - INTERVAL '2 days'
           ORDER BY po.token_issued_at"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 支払期限チェック対象を取得
pub async fn list_payment_check_reminders(pool: &PgPool) -> Result<Vec<PaymentCheckReminder>> {
    let rows = sqlx::query_as::<_, PaymentCheckReminder>(
        r#"SELECT pn.notice_id,
                  COALESCE(p.name, '') as partner_name,
                  (DATE(pn.created_at + INTERVAL '30 days') = CURRENT_DATE) as is_today
           FROM t_payment_notice pn
           JOIN t_purchase_order po ON po.id = pn.purchase_order_pk
           JOIN m_partner p ON p.partner_id = po.partner_id
           WHERE pn.partner_accepted_at IS NULL
             AND DATE(pn.created_at + INTERVAL '30 days') BETWEEN CURRENT_DATE - INTERVAL '1 day' AND CURRENT_DATE
           ORDER BY pn.created_at"#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 管理者通知メールアドレスを取得
pub async fn find_admin_notification_email(pool: &PgPool) -> Result<Option<String>> {
    let email: Option<(String,)> = sqlx::query_as(
        "SELECT email FROM s_user WHERE is_staff = TRUE AND email != '' LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(email.map(|(e,)| e))
}
