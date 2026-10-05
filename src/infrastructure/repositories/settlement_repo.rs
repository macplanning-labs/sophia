/// infrastructure/repositories/settlement_repo.rs — 月次確定・精算ダッシュボード用リポジトリ

use anyhow::Result;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::domain::services::settlement_dashboard::{SettlementRow, SettlementFilter};
use crate::domain::services::month_range::first_of_next_month;

/// 月次確定データをJOINクエリで取得
pub async fn list_settlement_rows(
    pool: &PgPool,
    target_month: NaiveDate,
    filter: &SettlementFilter,
) -> Result<Vec<SettlementRow>> {
    let month_start = target_month;
    let month_end = first_of_next_month(target_month)
        .ok_or_else(|| anyhow::anyhow!("年月の計算に失敗しました（オーバーフロー）"))?;

    let mut sql = String::from(r#"
        SELECT
            e.id AS engineer_id, e.name AS engineer_name, e.employee_id AS engineer_employee_code,
            p.partner_id, p.name AS partner_name,
            prj.project_id, prj.name AS project_name,
            cl.name AS client_name, cl.id AS client_id, cl.edi_system_type AS client_edi_system_type,
            cl.invoice_issued_by_client AS client_invoice_by_client,
            ts.id AS timesheet_id, ts.total_hours, ts.status AS timesheet_status,
            cc.id AS client_contract_id,
            -- 請求の条件は、その月の注文書（受注明細）から取る。注文書が無い月は、画面表示用に契約の値を
            -- 出すだけで、請求金額は 0 になり、請求書は作れない（received_order_id が NULL）
            rod.id AS received_order_id,
            COALESCE(rod.unit_price, cc.base_rate) AS billing_base_rate,
            COALESCE(rod.settlement_type, cc.settlement_type) AS billing_settlement_type,
            COALESCE(rod.lower_limit_hours, cc.lower_limit_hours) AS billing_lower_limit,
            COALESCE(rod.upper_limit_hours, cc.upper_limit_hours) AS billing_upper_limit,
            CASE WHEN rod.id IS NULL THEN cc.fixed_hours ELSE rod.fixed_hours END AS billing_fixed_hours,
            COALESCE(rod.deduction_rate, cc.deduction_rate) AS billing_deduction_rate,
            COALESCE(rod.overtime_rate, cc.overtime_rate) AS billing_overtime_rate,
            COALESCE(rod.man_month, cc.effort) AS billing_effort,
            cc.payment_terms AS billing_payment_terms,
            pc.id AS partner_contract_id, pc.base_rate AS payment_base_rate,
            pc.settlement_type AS payment_settlement_type,
            pc.lower_limit_hours AS payment_lower_limit,
            pc.upper_limit_hours AS payment_upper_limit,
            pc.fixed_hours AS payment_fixed_hours,
            pc.deduction_rate AS payment_deduction_rate,
            pc.overtime_rate AS payment_overtime_rate,
            pc.effort AS payment_effort,
            COALESCE(pc.payment_condition, '') AS payment_condition,
            -- 発行済みの判定: 請求書の明細（技術者ごと）で見る。請求書のヘッダが結びつく注文書は、まとめた請求書の
            -- 最初の1件だけなので、ヘッダだけで見ると、同じ請求書に入った2人目以降が「未発行」のままになり、
            -- 二重に請求できてしまう（フローテストで見つかった）
            (EXISTS(
                SELECT 1 FROM t_billing_invoice bi
                JOIN t_received_order ro ON bi.received_order_id = ro.id
                WHERE ro.client_contract_id = cc.id
                  AND ro.target_month = $1
            ) OR EXISTS(
                SELECT 1 FROM t_billing_invoice_item bii
                JOIN t_billing_invoice bi ON bi.id = bii.invoice_id
                WHERE bii.engineer_id = e.id
                  AND bi.client_id = cl.id
                  AND bi.target_month = $1
            )) AS invoice_issued,
            (CASE WHEN pc.id IS NULL THEN NULL ELSE EXISTS(
                SELECT 1 FROM t_payment_notice pn
                JOIN t_payment_notice_item pni ON pn.notice_id = pni.notice_id
                WHERE pni.partner_contract_id = pc.id
                  AND pn.target_month = $1
            ) END) AS notice_issued,
            po_info.order_id AS purchase_order_id,
            po_info.status AS purchase_order_status
        FROM m_engineer e
        JOIN m_client_contract cc ON cc.engineer_id = e.id
        JOIN m_project prj ON cc.project_id = prj.project_id
        JOIN m_client cl ON prj.client_id = cl.id
        LEFT JOIN LATERAL (
            SELECT pc2.*
              FROM m_partner_contract pc2
             WHERE pc2.engineer_id = e.id
               AND pc2.project_id = prj.project_id
               AND pc2.start_date < $2 AND pc2.end_date >= $1
             ORDER BY pc2.is_active DESC, pc2.id DESC
             LIMIT 1
        ) pc ON true
        LEFT JOIN m_partner p ON pc.partner_id = p.partner_id
        LEFT JOIN LATERAL (
            SELECT po.order_id, po.status
              FROM t_purchase_order po
             WHERE DATE_TRUNC('month', po.work_start)::date = $1
               AND (
                   po.partner_contract_id = pc.id
                   OR EXISTS (
                       SELECT 1 FROM t_purchase_order_item poi
                        WHERE poi.purchase_order_pk = po.id
                          AND poi.partner_contract_id = pc.id
                   )
               )
             ORDER BY po.updated_at DESC, po.order_id DESC
             LIMIT 1
        ) po_info ON pc.id IS NOT NULL
        LEFT JOIN LATERAL (
            SELECT ro.id, roi.unit_price, roi.man_month, roi.settlement_type,
                   roi.lower_limit_hours, roi.upper_limit_hours, roi.fixed_hours,
                   roi.deduction_rate, roi.overtime_rate
              FROM t_received_order ro
              JOIN t_received_order_item roi ON roi.order_id = ro.id
             WHERE ro.client_id = cl.id
               AND ro.target_month = $1
               AND ro.engineer_id = e.id
               AND ro.client_contract_id = cc.id
             ORDER BY ro.id DESC, roi.id
             LIMIT 1
        ) rod ON true
        LEFT JOIN t_monthly_timesheet ts
            ON ts.client_contract_id = cc.id
            AND ts.target_month = $1
        WHERE e.is_active = true
          AND cc.start_date < $2 AND cc.end_date >= $1
    "#);

    // 動的フィルタ
    let mut param_idx = 3;
    if filter.client_id.is_some() {
        sql.push_str(&format!(" AND cl.id = ${}", param_idx));
        param_idx += 1;
    }
    if filter.project_id.is_some() {
        sql.push_str(&format!(" AND prj.project_id = ${}", param_idx));
        param_idx += 1;
    }
    if filter.partner_id.is_some() {
        sql.push_str(&format!(" AND p.partner_id = ${}", param_idx));
        // param_idx += 1;
    }

    sql.push_str(" ORDER BY COALESCE(p.name, '自社'), e.name");

    let mut query = sqlx::query_as::<_, SettlementRow>(&sql)
        .bind(month_start)
        .bind(month_end);

    if let Some(client_id) = filter.client_id {
        query = query.bind(client_id);
    }
    if let Some(ref project_id) = filter.project_id {
        query = query.bind(project_id);
    }
    if let Some(ref partner_id) = filter.partner_id {
        query = query.bind(partner_id);
    }

    query.fetch_all(pool).await.map_err(|e| {
        tracing::error!("settlement_dashboard: fetch error: {:?}", e);
        anyhow::anyhow!("settlement_dashboard: fetch error: {:?}", e)
    })
}

/// 支払通知書承認閾値を取得（DB未設定時はNone、domain側でunwrap_or）
pub async fn get_notice_approval_threshold(pool: &PgPool) -> Result<Option<i32>> {
    let val: Option<i32> = sqlx::query_scalar(
        "SELECT notice_approval_threshold FROM s_company_info ORDER BY id LIMIT 1"
    )
    .fetch_one(pool)
    .await?;
    Ok(val)
}

/// トークン有効期限日数を取得（DB未設定時はNone、domain側でunwrap_or）
pub async fn get_token_expiry_days(pool: &PgPool) -> Result<Option<i32>> {
    let val: Option<i32> = sqlx::query_scalar(
        "SELECT token_expiry_days FROM s_company_info ORDER BY id LIMIT 1"
    )
    .fetch_one(pool)
    .await?;
    Ok(val)
}

/// 支払通知書の最大通番を取得（prefix マッチ）
pub async fn max_notice_id_with_prefix(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
) -> Result<Option<String>> {
    let val: Option<String> = sqlx::query_scalar(
        "SELECT MAX(notice_id) FROM t_payment_notice WHERE notice_id LIKE $1"
    )
    .bind(format!("{}%", prefix))
    .fetch_one(&mut **tx)
    .await?;
    Ok(val)
}

/// 請求書の最大通番を取得（prefix マッチ）
pub async fn max_invoice_no_with_prefix(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    prefix: &str,
) -> Result<Option<String>> {
    let val: Option<String> = sqlx::query_scalar(
        "SELECT MAX(invoice_no) FROM t_billing_invoice WHERE invoice_no LIKE $1"
    )
    .bind(format!("{}%", prefix))
    .fetch_one(&mut **tx)
    .await?;
    Ok(val)
}

/// 採番用の排他ロック（トランザクション終了まで保持）。同じ key の採番は直列になり、
/// 後から来た側は先の確定後の最大値を読む。
pub async fn lock_numbering(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    key: &str,
) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(key)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// ── 支払通知書 INSERT ──

/// 支払通知書ヘッダを作成
pub async fn insert_payment_notice(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    notice_id: &str,
    purchase_order_id: &str,
    partner_id: &str,
    target_month: NaiveDate,
    payment_due_date: NaiveDate,
    subtotal: i32,
    tax_amount: i32,
    total: i32,
    approval_status: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_payment_notice (
            notice_id, uuid, purchase_order_id, partner_id, target_month,
            notice_date, payment_due_date, subtotal, tax_amount, total, approval_status,
            purchase_order_pk
        ) VALUES ($1, gen_random_uuid(), $2, $3, $4, CURRENT_DATE, $5, $6, $7, $8, $9,
                  (SELECT id FROM t_purchase_order WHERE order_id = $2))
        "#
    )
    .bind(notice_id)
    .bind(purchase_order_id)
    .bind(partner_id)
    .bind(target_month)
    .bind(payment_due_date)
    .bind(subtotal)
    .bind(tax_amount)
    .bind(total)
    .bind(approval_status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 支払通知書明細を作成
pub async fn insert_payment_notice_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    notice_id: &str,
    partner_contract_id: i64,
    actual_hours: Decimal,
    base_fee: i32,
    effort: Decimal,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    fixed_hours: Option<Decimal>,
    deduction_rate: i32,
    overtime_rate: i32,
    adjustment: i32,
    amount: i32,
    tax_rate: Decimal,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_payment_notice_item (
            notice_id, partner_contract_id, actual_hours, base_fee,
            effort, lower_limit_hours, upper_limit_hours, fixed_hours,
            deduction_rate, overtime_rate, adjustment, amount, tax_rate
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#
    )
    .bind(notice_id)
    .bind(partner_contract_id)
    .bind(actual_hours)
    .bind(base_fee)
    .bind(effort)
    .bind(lower_limit_hours)
    .bind(upper_limit_hours)
    .bind(fixed_hours)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(adjustment)
    .bind(amount)
    .bind(tax_rate)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 発注書のステータスを NOTICE_CREATED に更新
pub async fn update_purchase_order_status_to_notice_created(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    order_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE t_purchase_order SET status = 'NOTICE_CREATED', updated_at = NOW() \
         WHERE order_id = $1 AND status IN ('ACCEPTED', 'REPORT_RECEIVED')"
    )
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ── 請求書 INSERT ──

/// 請求書ヘッダを作成して ID を返す
pub async fn insert_invoice_header(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invoice_no: &str,
    client_id: i64,
    target_month: NaiveDate,
    month_end: NaiveDate,
    subject: &str,
    subtotal: i32,
    tax_amount: i32,
    total: i32,
    due_date: NaiveDate,
    project_id: Option<&str>,
    billing_unit: &str,
) -> Result<i64> {
    let invoice_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_billing_invoice (
            invoice_no, client_id, target_month, work_start, work_end,
            subject, subtotal, tax_amount, total, due_date, source, status,
            project_id, billing_unit
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'SELF', 'PENDING_APPROVAL', $11, $12)
        RETURNING id
        "#
    )
    .bind(invoice_no)
    .bind(client_id)
    .bind(target_month)
    .bind(target_month)
    .bind(month_end)
    .bind(subject)
    .bind(subtotal)
    .bind(tax_amount)
    .bind(total)
    .bind(due_date)
    .bind(project_id)
    .bind(billing_unit)
    .fetch_one(&mut **tx)
    .await?;
    Ok(invoice_id)
}

/// 請求書明細を作成
pub async fn insert_invoice_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invoice_id: i64,
    engineer_id: i64,
    description: &str,
    quantity: Decimal,
    unit_price: i32,
    amount: i32,
    settlement_type: &str,
    lower_limit: Decimal,
    upper_limit: Decimal,
    deduction_rate: i32,
    overtime_rate: i32,
    tax_rate: Decimal,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_billing_invoice_item (
            invoice_id, engineer_id, description, quantity, unit_price,
            amount, settlement_type, lower_limit, upper_limit,
            deduction_rate, overtime_rate, tax_rate
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#
    )
    .bind(invoice_id)
    .bind(engineer_id)
    .bind(description)
    .bind(quantity)
    .bind(unit_price)
    .bind(amount)
    .bind(settlement_type)
    .bind(lower_limit)
    .bind(upper_limit)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(tax_rate)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 受注と請求書を紐付け（受注のステータスを INVOICED に進める）
pub async fn link_received_order_invoice(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    received_order_id: i64,
) -> Result<()> {
    // 請求書と受注の紐づけは、請求書側（t_billing_invoice.received_order_id）に持つ。
    // t_received_order に invoice_id 列は無い（2026-09-01 の整理で、存在しない列への書き込みが混入し、
    // 請求書の一括発行が失敗していた）。ここでは受注のステータスだけを進める。
    sqlx::query(
        "UPDATE t_received_order SET status = 'INVOICED', updated_at = NOW() WHERE id = $1 AND status IN ('REGISTERED', 'REPORT_RECEIVED', 'REPORT_SENT')"
    )
    .bind(received_order_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 請求書ヘッダに受注を紐付け
pub async fn set_invoice_received_order(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invoice_id: i64,
    received_order_id: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE t_billing_invoice SET received_order_id = $1 WHERE id = $2"
    )
    .bind(received_order_id)
    .bind(invoice_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 対象月の受注を検索
pub async fn find_received_order_for_invoice(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    client_id: i64,
    target_month: NaiveDate,
    engineer_id: i64,
    client_contract_id: i64,
) -> Result<Option<i64>> {
    let ro_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM t_received_order WHERE client_id = $1 AND target_month = $2 AND engineer_id = $3 AND client_contract_id = $4"
    )
    .bind(client_id)
    .bind(target_month)
    .bind(engineer_id)
    .bind(client_contract_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(ro_id)
}

/// IDで請求書を検索（読み直し用）
pub async fn find_invoices_by_ids(pool: &PgPool, ids: &[i64]) -> Result<Vec<(i64, String, i64, i32)>> {
    let rows = sqlx::query_as::<_, (i64, String, i64, i32)>(
        "SELECT id, invoice_no, client_id, total FROM t_billing_invoice WHERE id = ANY($1)"
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// IDで支払通知を検索（読み直し用）
pub async fn find_payment_notices_by_ids(pool: &PgPool, notice_ids: &[String]) -> Result<Vec<(String, i32)>> {
    let rows = sqlx::query_as::<_, (String, i32)>(
        "SELECT notice_id, total FROM t_payment_notice WHERE notice_id = ANY($1)"
    )
    .bind(notice_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 取引先ごとの請求単位(m_client.billing_unit)を取得する。取引先IDの集合に対して1回で引く。
pub async fn client_billing_units(
    pool: &PgPool,
    client_ids: &[i64],
) -> Result<std::collections::HashMap<i64, String>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, billing_unit FROM m_client WHERE id = ANY($1)"
    )
    .bind(client_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// 強制確定の履歴を1行追加する(確定と同じトランザクション内)
pub async fn insert_force_confirm(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    invoice_id: i64,
    client_contract_id: i64,
    action: &str,
    reason: &str,
    created_by_id: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO t_billing_force_confirm (invoice_id, client_contract_id, action, reason, created_by_id)
         VALUES ($1, $2, $3, $4, $5)"
    )
    .bind(invoice_id)
    .bind(client_contract_id)
    .bind(action)
    .bind(reason)
    .bind(created_by_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
