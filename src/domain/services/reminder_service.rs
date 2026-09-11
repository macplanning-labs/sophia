/// domain/services/reminder_service.rs — リマインドメール送信サービス
///
/// EDI互換: send_report_reminders, send_approval_reminders, send_deadline_reminders
///
/// 3種類のリマインドを一括送信する:
/// 1. 注文書承諾リマインド — SENT状態で3日以上経過した注文書
/// 2. 稼働報告提出リマインド — PENDING状態のREPORT_UPLOADタスク
/// 3. 請求書承諾リマインド — 送付済みで未承諾の支払通知書

use anyhow::Result;
use sqlx::PgPool;
use crate::domain::services::email_service::{EmailService, compose_work_report_reminder_email};
use crate::infrastructure::repositories::{order_repo, task_repo, reminder_repo};

/// 全リマインドを一括送信
///
/// 3種類は互いに独立した機能のため、1つがエラーになっても残りは実行する。
pub async fn send_all_reminders(pool: &PgPool, dry_run: bool) -> Result<()> {
    let mut total_sent = 0;

    // 1. 注文書承諾リマインド
    match send_order_approve_reminders(pool, dry_run).await {
        Ok(n) => total_sent += n,
        Err(e) => tracing::error!("[注文書承諾リマインド] 実行エラー: {:?}", e),
    }

    // 2. 稼働報告提出リマインド
    match send_work_report_reminders(pool, dry_run).await {
        Ok(n) => total_sent += n,
        Err(e) => tracing::error!("[稼働報告提出リマインド] 実行エラー: {:?}", e),
    }

    // 3. 請求書承諾リマインド
    match send_invoice_approve_reminders(pool, dry_run).await {
        Ok(n) => total_sent += n,
        Err(e) => tracing::error!("[請求書承諾リマインド] 実行エラー: {:?}", e),
    }

    // 4. 稼働報告 初回依頼 送り忘れ防止の社内通知
    match send_work_report_request_notifications(pool, dry_run).await {
        Ok(n) => total_sent += n,
        Err(e) => tracing::error!("[稼働報告初回依頼 社内通知] 実行エラー: {:?}", e),
    }

    if dry_run {
        tracing::info!("📧 ドライラン完了（送信対象: {}件）", total_sent);
    } else {
        tracing::info!("✅ リマインド送信完了: {}件", total_sent);
    }

    Ok(())
}

/// 稼働報告催促メールを1通送信する（自動リマインドジョブ専用）
///
/// `token_url` はログイン不要の `/token/{uuid}` ページ（稼働報告アップロード機能付き）を
/// 呼び出し側で組み立てて渡す。パートナーポータルへのログインを要求する裸のURLにしない。
/// `engineer_names` は未登録エンジニアを「、」区切りで連結した文字列（複数名は1通にまとめる）。
pub async fn send_work_report_request_email(
    pool: &PgPool,
    engineer_names: &str,
    partner_email: &str,
    month_short: &str,
    token_url: &str,
    deadline: chrono::NaiveDate,
) -> Result<(), crate::domain::services::email_service::EmailError> {
    let ctx = compose_work_report_reminder_email(engineer_names, month_short, token_url, deadline);
    EmailService::new(pool.clone())
        .send_by_template("work_report_reminder", partner_email, None, &ctx)
        .await
}

/// 注文書承諾リマインド — SENT状態で3日以上経過した注文書のパートナーに催促
async fn send_order_approve_reminders(pool: &PgPool, dry_run: bool) -> Result<usize> {
    let orders = reminder_repo::list_pending_order_reminders(pool).await?;

    if orders.is_empty() {
        tracing::info!("  [注文書承諾リマインド] 対象なし");
        return Ok(0);
    }

    tracing::info!("  [注文書承諾リマインド] 対象: {}件", orders.len());

    let email_svc = EmailService::new(pool.clone());
    let mut sent = 0;

    for order in &orders {
        tracing::info!("    {} / {} / {}日経過",
            order.partner_name, order.order_id, order.days_pending);

        if dry_run {
            continue;
        }

        let base_url = std::env::var("BASE_URL").unwrap_or_default();
        let token_url = format!("{}/token/{}", base_url, order.uuid);
        let ctx = crate::domain::services::email_service::compose_order_approve_reminder_email(
            &order.partner_name, &order.order_id, order.days_pending, &token_url,
        );

        if let Err(e) = email_svc.send_by_template(
            "order_approve_reminder", &order.partner_email, None, &ctx,
        ).await {
            tracing::error!("    → 送信失敗: {:?}", e);
        } else {
            tracing::info!("    → 送信完了: {}", order.partner_email);
            sent += 1;
        }
    }

    Ok(sent)
}

/// 稼働報告提出リマインドを送るのは提出期限の何日前からか（期限を過ぎたタスクは常に対象）
const REPORT_REMINDER_LEAD_DAYS: i32 = 3;

/// 稼働報告提出リマインド — 提出期限が REPORT_REMINDER_LEAD_DAYS 日以内に迫った/過ぎたPENDING状態のREPORT_UPLOADタスク
async fn send_work_report_reminders(pool: &PgPool, dry_run: bool) -> Result<usize> {
    let reports = task_repo::list_pending_report_reminders(pool, REPORT_REMINDER_LEAD_DAYS).await?;

    if reports.is_empty() {
        tracing::info!("  [稼働報告リマインド] 対象なし");
        return Ok(0);
    }

    tracing::info!("  [稼働報告リマインド] 対象: {}件", reports.len());

    let mut sent = 0;

    for report in &reports {
        tracing::info!("    {} / {}", report.partner_name, report.target_month);

        if dry_run {
            continue;
        }

        let order_uuid = match order_repo::find_purchase_order_uuid_by_contract_month(
            pool, report.partner_contract_id, report.work_month,
        ).await {
            Ok(Some(uuid)) => uuid,
            Ok(None) => {
                tracing::warn!(
                    "    → リンク生成用の発注書が見つからずスキップ: partner_contract_id={}, month={}",
                    report.partner_contract_id, report.work_month
                );
                continue;
            }
            Err(e) => {
                tracing::error!("    → 発注書UUID取得エラー: {:?}", e);
                continue;
            }
        };
        let base_url = std::env::var("BASE_URL").unwrap_or_default();
        let token_url = format!("{}/token/{}", base_url.trim_end_matches('/'), order_uuid);
        let month_short = report.work_month.format("%m月").to_string();
        let engineer_names_display = report.engineer_names
            .split('、')
            .filter(|n| !n.is_empty())
            .map(|n| format!("{}さん", n))
            .collect::<Vec<_>>()
            .join("、");

        if let Err(e) = send_work_report_request_email(
            pool,
            &engineer_names_display,
            &report.partner_email,
            &month_short,
            &token_url,
            report.deadline,
        ).await {
            tracing::error!("    → 送信失敗: {:?}", e);
            continue;
        }

        if let Err(e) = task_repo::mark_report_reminder_sent(
            pool, &report.partner_id, report.work_month,
        ).await {
            tracing::error!("    → reminder_sent更新失敗: {:?}", e);
        }

        tracing::info!("    → 送信完了: {}", report.partner_email);
        sent += 1;
    }

    Ok(sent)
}

/// 請求書承諾リマインド — 送付済みで未承諾の支払通知書
async fn send_invoice_approve_reminders(pool: &PgPool, dry_run: bool) -> Result<usize> {
    let invoices = reminder_repo::list_pending_invoice_reminders(pool).await?;

    if invoices.is_empty() {
        tracing::info!("  [請求書承諾リマインド] 対象なし");
        return Ok(0);
    }

    tracing::info!("  [請求書承諾リマインド] 対象: {}件", invoices.len());

    let email_svc = EmailService::new(pool.clone());
    let mut sent = 0;

    for inv in &invoices {
        tracing::info!("    {} / {}", inv.partner_name, inv.notice_id);

        if dry_run {
            continue;
        }

        let base_url = std::env::var("BASE_URL").unwrap_or_default();
        let token_url = format!("{}/token/{}", base_url, inv.uuid);
        let ctx = crate::domain::services::email_service::compose_invoice_approve_reminder_email(
            &inv.partner_name, &inv.notice_id,
            &inv.target_month.format("%Y年%m月").to_string(), inv.total as i64, &token_url,
        );

        if let Err(e) = email_svc.send_by_template(
            "invoice_approve_reminder", &inv.partner_email, None, &ctx,
        ).await {
            tracing::error!("    → 送信失敗: {:?}", e);
        } else {
            tracing::info!("    → 送信完了: {}", inv.partner_email);
            sent += 1;
        }
    }

    Ok(sent)
}

/// 稼働報告 初回依頼メールの送り忘れ防止 社内通知
///
/// 案件の `report_request_day` が本日と一致する、SENT/ACCEPTED状態・当月分の発注書に対し、
/// パートナーへ直接送るのではなく、社内（`get_notify_email`の宛先）へ
/// 「そろそろ手動で送るタイミングです」と通知する。
/// クロス等から受け取ったPDFを添付して手動送信する運用のため、自動送信はしない
/// （詳細は migrations/041_work_report_request_auto_send.sql 参照）。
async fn send_work_report_request_notifications(pool: &PgPool, dry_run: bool) -> Result<usize> {
    use chrono::Datelike;
    use crate::domain::services::email_service::{compose_work_report_request_internal_notify_email, get_notify_email};

    let today_day = chrono::Utc::now().day() as i32;
    let targets = order_repo::list_report_request_notify_targets(pool, today_day).await?;

    if targets.is_empty() {
        tracing::info!("  [稼働報告初回依頼 社内通知] 対象なし");
        return Ok(0);
    }

    tracing::info!("  [稼働報告初回依頼 社内通知] 対象: {}件", targets.len());

    let email_svc = EmailService::new(pool.clone());
    let notify_email = get_notify_email(pool).await;
    let mut sent = 0;

    for target in &targets {
        tracing::info!("    {} / {} / {}", target.partner_name, target.project_name, target.order_id);

        if dry_run {
            continue;
        }

        let base_url = std::env::var("BASE_URL").unwrap_or_default();
        let order_url = format!("{}/orders/{}", base_url.trim_end_matches('/'), target.order_id);
        let month = target.work_month.format("%Y年%m月").to_string();
        let ctx = compose_work_report_request_internal_notify_email(
            &target.partner_name, &target.project_name, &target.order_id, &month, &order_url,
        );

        if let Err(e) = email_svc.send_by_template(
            "work_report_request_internal_notify", &notify_email, None, &ctx,
        ).await {
            tracing::error!("    → 送信失敗: {:?}", e);
            continue;
        }

        if let Err(e) = order_repo::mark_report_request_notified(pool, &target.order_id).await {
            tracing::error!("    → report_request_notified_at更新失敗: {:?}", e);
        }

        tracing::info!("    → 通知完了: {}", notify_email);
        sent += 1;
    }

    Ok(sent)
}

/// 期限超過チェック（管理者通知）
/// EDI互換: tasks/management/commands/check_overdue_tasks.py
///
/// 以下をチェックし、警告があれば管理者にメール通知する:
/// - 注文書が未承諾（発行から2日以上経過）
/// - 支払期限が当日 or 翌日（1日超過）
pub async fn check_overdue_tasks(pool: &PgPool, dry_run: bool) -> Result<()> {
    let mut warnings: Vec<String> = Vec::new();

    // 1. 未承諾注文書（SENT状態で2日以上経過）
    let overdue_orders = reminder_repo::list_overdue_orders(pool).await?;

    for o in &overdue_orders {
        let work_period = match (&o.work_start, &o.work_end) {
            (Some(s), Some(e)) => format!("{} 〜 {}", s, e),
            _ => "未設定".into(),
        };
        warnings.push(format!(
            "⚠️ 注文書未承諾: {} ({}) — 発行から{}日経過 / 作業期間 {}",
            o.order_id, o.partner_name, o.days_pending, work_period
        ));
    }

    // 2. 支払期限チェック（当日 or 翌日のみ — 支払通知書の承諾期限）
    let payment_checks = reminder_repo::list_payment_check_reminders(pool).await?;

    for pc in &payment_checks {
        if pc.is_today {
            warnings.push(format!(
                "💰 支払期限当日: {} ({}) — 本日が支払期限です",
                pc.notice_id, pc.partner_name
            ));
        } else {
            warnings.push(format!(
                "🚨 支払期限超過: {} ({}) — 期限翌日です。未払いの場合は対応をお願いします",
                pc.notice_id, pc.partner_name
            ));
        }
    }

    if warnings.is_empty() {
        tracing::info!("  [期限超過チェック] 警告事項なし");
        return Ok(());
    }

    tracing::info!("  [期限超過チェック] 警告: {}件", warnings.len());
    for w in &warnings {
        tracing::warn!("    {}", w);
    }

    if dry_run {
        tracing::info!("  [期限超過チェック] ドライラン — メール送信スキップ");
        return Ok(());
    }

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // 管理者にメール送信
    let admin_email = reminder_repo::find_admin_notification_email(pool).await?;

    if let Some(email) = admin_email {
        let subject = format!("【Sophia】毎朝チェック: {}件の警告があります ({})", warnings.len(), today);
        let body = format!(
            "管理者各位\n\n{} の定期チェックで以下の警告があります。\n\n{}\n\n確認・対応をお願いします。\n",
            today,
            warnings.iter().map(|w| format!("  {}", w)).collect::<Vec<_>>().join("\n")
        );

        let email_svc = EmailService::new(pool.clone());
        if let Err(e) = email_svc.send(&email, None, &subject, &body).await {
            tracing::error!("  [期限超過チェック] メール送信失敗: {:?}", e);
        } else {
            tracing::info!("  [期限超過チェック] メール送信完了: {}", email);
        }
    }

    // Google Chat Incoming Webhook(任意)。URLはログに出さない([SEC-13.2])
    post_google_chat_webhook(&warnings, &today).await;

    Ok(())
}

/// Google Chat Incoming Webhook へ警告一覧を投稿する(実体は`chat_notifier::post`に共通化)。
async fn post_google_chat_webhook(warnings: &[String], today: &str) {
    let text = format!(
        "*【Sophia】毎朝チェック: {}件の警告 ({})*\n{}",
        warnings.len(),
        today,
        warnings
            .iter()
            .map(|w| format!("• {}", w))
            .collect::<Vec<_>>()
            .join("\n")
    );

    super::chat_notifier::post(&text).await;
}
