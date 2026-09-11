/// domain/services/rollforward.rs — ロールフォワードサービス
///
/// 受注（ReceivedOrder）を翌月にコピーする機能。
/// Django版 client_contract/application/services/received_order_service.py の完全移植。

use anyhow::{Result, bail};
use chrono::{NaiveDate, Datelike};
use sqlx::PgPool;
use tracing::info;

/// 受注を翌月にロールフォワード（コピー）する
pub async fn rollforward_order(pool: &PgPool, source_order_id: i64) -> Result<i64> {
    use crate::infrastructure::repositories::order_repo;

    // 元の受注を取得
    let source = order_repo::find_received_order(pool, source_order_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("受注が見つかりません: {}", source_order_id))?;

    // 翌月計算
    let (next_year, next_month) = if source.target_month.month() == 12 {
        (source.target_month.year() + 1, 1u32)
    } else {
        (source.target_month.year(), source.target_month.month() + 1)
    };

    let next_target = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;

    let last_day = last_day_of_month(next_year, next_month);
    let next_work_end = NaiveDate::from_ymd_opt(next_year, next_month, last_day)
        .ok_or_else(|| anyhow::anyhow!("Invalid date"))?;

    // 重複チェック
    let exists = order_repo::received_order_exists_for_client_month_project(
        pool,
        source.client_id,
        next_target,
        &source.project_name,
    )
    .await?;

    if exists {
        bail!(
            "{} の {} 分は既に存在します",
            source.project_name,
            next_target.format("%Y/%m")
        );
    }

    // トランザクション開始
    let mut tx = pool.begin().await?;

    // 受注書番号を採番（RO-YYYYMM-NNN、api_create と同じ方式）
    let month_str = next_target.format("%Y%m").to_string();
    let existing_count = order_repo::count_received_orders_with_prefix(
        &mut tx, &format!("RO-{}-%", month_str),
    ).await?;
    let received_order_no = format!("RO-{}-{:03}", month_str, existing_count + 1);

    // 受注コピー
    let parent_id = source.parent_order_id.unwrap_or(source.id);
    let remarks = format!("ロールフォワード（元: {}）", source.target_month.format("%Y/%m"));
    let new_id = order_repo::insert_rollforward_received_order(
        &mut tx,
        &received_order_no,
        source.client_id,
        source.engineer_id,
        source.client_contract_id,
        if source.client_order_number.is_empty() { None } else { Some(source.client_order_number.as_str()) },
        next_target,
        next_work_end,
        &source.project_name,
        source.is_recurring,
        Some(parent_id),
        &remarks,
        if source.report_to_email.is_empty() { None } else { Some(source.report_to_email.as_str()) },
        if source.report_cc_emails.is_empty() { None } else { Some(source.report_cc_emails.as_str()) },
        if source.invoice_to_email.is_empty() { None } else { Some(source.invoice_to_email.as_str()) },
        if source.invoice_cc_emails.is_empty() { None } else { Some(source.invoice_cc_emails.as_str()) },
    ).await?;

    // 明細コピー
    let items = order_repo::list_received_order_items_tx(&mut tx, source.id).await?;

    for item in &items {
        order_repo::insert_rollforward_received_order_item(
            &mut tx,
            new_id,
            item.client_contract_id,
            &item.engineer_name,
            item.unit_price,
            item.man_month,
            &item.settlement_type,
            item.base_rate,
            item.lower_limit_hours,
            item.upper_limit_hours,
            item.fixed_hours,
            item.deduction_rate,
            item.overtime_rate,
            item.effort,
        )
        .await?;
    }

    tx.commit().await?;

    info!(
        "ロールフォワード完了: order_id={} → new_id={}",
        source.id, new_id
    );

    Ok(new_id)
}

/// 継続注文（is_recurring=true）の一括ロールフォワード
pub async fn rollforward_all_recurring(pool: &PgPool) -> Result<Vec<(i64, i64)>> {
    use crate::infrastructure::repositories::order_repo;

    // クライアント×プロジェクトごとに最新のものだけ処理
    let recurring_orders = order_repo::list_recurring_received_orders(pool).await?;

    let mut results = Vec::new();
    for order in &recurring_orders {
        match rollforward_order(pool, order.id).await {
            Ok(new_id) => results.push((order.id, new_id)),
            Err(e) => info!("スキップ: {}", e),
        }
    }

    Ok(results)
}

// ── ヘルパー ──

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    next.unwrap().pred_opt().unwrap().day()
}

