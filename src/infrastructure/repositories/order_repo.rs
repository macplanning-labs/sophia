/// infrastructure/repositories/order_repo.rs — 注文書・受注・支払通知CRUD

use anyhow::Result;
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::domain::models::partner_contract::*;
use crate::domain::models::client_contract::*;
use crate::domain::models::project::*;
use crate::domain::models::engineer::Engineer;

/// パートナー契約一覧
pub async fn list_partner_contracts(pool: &PgPool) -> Result<Vec<PartnerContract>> {
    let rows = sqlx::query_as::<_, PartnerContract>(
        "SELECT * FROM m_partner_contract WHERE is_active = true ORDER BY id DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 注文書一覧
pub async fn list_purchase_orders(pool: &PgPool) -> Result<Vec<PurchaseOrder>> {
    let rows = sqlx::query_as::<_, PurchaseOrder>(
        "SELECT * FROM t_purchase_order ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 注文書取得（order_id指定）
pub async fn find_purchase_order(pool: &PgPool, order_id: &str) -> Result<Option<PurchaseOrder>> {
    let row = sqlx::query_as::<_, PurchaseOrder>(
        "SELECT * FROM t_purchase_order WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 注文書取得（UUID指定 — トークンアクセス用）
pub async fn find_purchase_order_by_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<Option<PurchaseOrder>> {
    let row = sqlx::query_as::<_, PurchaseOrder>(
        "SELECT * FROM t_purchase_order WHERE uuid = $1"
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナー契約×対象月に対応する発注書UUIDを取得する（稼働報告提出リマインドのリンク生成用）。
/// 同月に複数あれば最新（作成日時降順）を採用する。
pub async fn find_purchase_order_uuid_by_contract_month(
    pool: &PgPool,
    partner_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<Option<uuid::Uuid>> {
    let row: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT uuid FROM t_purchase_order \
         WHERE partner_contract_id = $1 \
           AND DATE_TRUNC('month', work_start) = DATE_TRUNC('month', $2::date) \
         ORDER BY created_at DESC LIMIT 1"
    )
    .bind(partner_contract_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 注文書明細一覧
pub async fn list_order_items(pool: &PgPool, order_id: &str) -> Result<Vec<PurchaseOrderItem>> {
    let rows = sqlx::query_as::<_, PurchaseOrderItem>(
        "SELECT * FROM t_purchase_order_item WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 支払通知書一覧
pub async fn list_payment_notices(pool: &PgPool) -> Result<Vec<PaymentNotice>> {
    let rows = sqlx::query_as::<_, PaymentNotice>(
        "SELECT * FROM t_payment_notice ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 受注注文書一覧
pub async fn list_received_orders(pool: &PgPool) -> Result<Vec<ReceivedOrder>> {
    let rows = sqlx::query_as::<_, ReceivedOrder>(
        "SELECT * FROM t_received_order ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 顧客契約一覧
pub async fn list_client_contracts(pool: &PgPool) -> Result<Vec<ClientContract>> {
    let rows = sqlx::query_as::<_, ClientContract>(
        "SELECT * FROM m_client_contract WHERE is_active = true ORDER BY id DESC"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 案件一覧（クライアント名結合）
pub async fn list_projects(pool: &PgPool) -> Result<Vec<ProjectWithClient>> {
    let rows = sqlx::query_as::<_, ProjectWithClient>(
        r#"
        SELECT p.*, c.name as client_name
        FROM m_project p
        JOIN m_client c ON p.client_id = c.id
        ORDER BY p.created_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// ステータス別注文書件数
pub async fn count_orders_by_status(pool: &PgPool, status: &str) -> Result<i64> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM t_purchase_order WHERE status = $1"
    )
    .bind(status)
    .fetch_one(pool)
    .await?;
    Ok(count.0)
}

// ── 注文書ステータス変更・削除（2026-07-12追加。P2-3続き:
//    presentation/handlers/orders.rs 直書きSQLのRepository層移行）──

/// 注文書のステータスのみ取得（DRAFT/SENTチェック等の軽量な事前確認用）
pub async fn find_purchase_order_status(pool: &PgPool, order_id: &str) -> Result<Option<String>> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM t_purchase_order WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;
    Ok(status)
}

/// 受注書のステータスのみ取得（REGISTEREDチェック等の軽量な事前確認用）
pub async fn find_received_order_status(pool: &PgPool, id: i64) -> Result<Option<String>> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM t_received_order WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(status)
}

/// 注文書のステータスを更新する（更新件数を返す。0件なら対象が存在しない）
pub async fn update_purchase_order_status(pool: &PgPool, order_id: &str, status: &str) -> Result<u64> {
    let result = sqlx::query("UPDATE t_purchase_order SET status = $1, updated_at = NOW() WHERE order_id = $2")
        .bind(status)
        .bind(order_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 稼働報告の「受領」時点で進めてよい発注ステータスか（REPORT_RECEIVED より前のみ）。
/// NOTICE_CREATED 以降へは進めない（回帰防止）。
pub fn is_pre_report_received_po_status(status: &str) -> bool {
    matches!(status, "DRAFT" | "SENT" | "ACCEPTED")
}

/// クライアント向け請求書のメール送信完了に伴い、紐づく受注を INVOICED → INVOICE_SENT へ進める。
/// 発注側の NOTICE_CREATED → NOTICE_CONFIRMED と同じ粒度に揃えたもの（2026-08-18追加）。
/// 現在のステータスが INVOICED の場合のみ更新する（回帰防止・二重送信での巻き戻り防止）。
pub async fn mark_received_order_invoice_sent(pool: &PgPool, received_order_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_received_order SET status = 'INVOICE_SENT', updated_at = NOW() WHERE id = $1 AND status = 'INVOICED'"
    )
    .bind(received_order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// クライアントの請求書受領確認完了に伴い、紐づく受注を INVOICE_SENT → INVOICE_CONFIRMED へ進める。
/// 現在のステータスが INVOICE_SENT の場合のみ更新する（回帰防止）。
pub async fn mark_received_order_invoice_confirmed(pool: &PgPool, received_order_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_received_order SET status = 'INVOICE_CONFIRMED', updated_at = NOW() WHERE id = $1 AND status = 'INVOICE_SENT'"
    )
    .bind(received_order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 稼働報告受領に伴い、対応する発注・受注を `REPORT_RECEIVED` へ進める。
///
/// - 発注: 受注契約のエンジニアに紐づき、対象月（`work_start` の月初）が一致し、
///   かつ現ステータスが DRAFT/SENT/ACCEPTED のもののみ
/// - 受注: 同一契約ID、または 技術者+対象月+クライアント+案件名 が一致し REGISTERED のもの
///   （未紐付けならこのタイミングで `client_contract_id` も埋める）
///
/// 戻り値: `(発注更新件数, 受注更新件数)`。対象なしは `(0, 0)`。
pub async fn advance_to_report_received_for_timesheet(
    pool: &PgPool,
    client_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<(u64, u64)> {
    let po_result = sqlx::query(
        r#"
        UPDATE t_purchase_order po
           SET status = 'REPORT_RECEIVED', updated_at = NOW()
         WHERE po.status IN ('DRAFT', 'SENT', 'ACCEPTED')
           AND DATE_TRUNC('month', po.work_start)::date = $2
           AND EXISTS (
               SELECT 1
                 FROM m_client_contract cc
                WHERE cc.id = $1
                  AND (
                      po.engineer_id = cc.engineer_id
                      OR EXISTS (
                          SELECT 1 FROM m_partner_contract pc
                           WHERE pc.id = po.partner_contract_id
                             AND pc.engineer_id = cc.engineer_id
                      )
                      OR EXISTS (
                          SELECT 1
                            FROM t_purchase_order_item poi
                            JOIN m_partner_contract pc ON pc.id = poi.partner_contract_id
                           WHERE poi.purchase_order_pk = po.id
                             AND pc.engineer_id = cc.engineer_id
                      )
                  )
           )
        "#,
    )
    .bind(client_contract_id)
    .bind(target_month)
    .execute(pool)
    .await?;

    let ro_result = sqlx::query(
        r#"
        UPDATE t_received_order ro
           SET status = 'REPORT_RECEIVED',
               client_contract_id = COALESCE(ro.client_contract_id, cc.id),
               updated_at = NOW()
          FROM m_client_contract cc
          JOIN m_project pr ON pr.project_id = cc.project_id
         WHERE cc.id = $1
           AND ro.target_month = $2
           AND ro.status = 'REGISTERED'
           AND ro.client_id = pr.client_id
           AND (
                ro.client_contract_id = cc.id
                OR (
                    ro.engineer_id = cc.engineer_id
                    AND (
                        BTRIM(COALESCE(ro.project_name, '')) = BTRIM(pr.name)
                        OR (
                            NULLIF(BTRIM(COALESCE(pr.edi_project_alias, '')), '') IS NOT NULL
                            AND BTRIM(ro.project_name) = BTRIM(pr.edi_project_alias)
                        )
                    )
                )
           )
        "#,
    )
    .bind(client_contract_id)
    .bind(target_month)
    .execute(pool)
    .await?;

    // 明細側も未紐付けなら契約IDを埋める
    let _ = sqlx::query(
        r#"
        UPDATE t_received_order_item roi
           SET client_contract_id = ro.client_contract_id
          FROM t_received_order ro
         WHERE roi.order_id = ro.id
           AND ro.client_contract_id = $1
           AND ro.target_month = $2
           AND roi.client_contract_id IS NULL
        "#,
    )
    .bind(client_contract_id)
    .bind(target_month)
    .execute(pool)
    .await;

    Ok((po_result.rows_affected(), ro_result.rows_affected()))
}

/// 受注書のステータスを更新する（id数値 or received_order_no文字列のどちらでも検索。更新件数を返す）
pub async fn update_received_order_status_by_id_or_no(pool: &PgPool, id_or_no: &str, status: &str) -> Result<u64> {
    let result = if let Ok(num_id) = id_or_no.parse::<i64>() {
        sqlx::query("UPDATE t_received_order SET status = $1, updated_at = NOW() WHERE id = $2")
            .bind(status)
            .bind(num_id)
            .execute(pool)
            .await?
    } else {
        sqlx::query("UPDATE t_received_order SET status = $1, updated_at = NOW() WHERE received_order_no = $2")
            .bind(status)
            .bind(id_or_no)
            .execute(pool)
            .await?
    };
    Ok(result.rows_affected())
}

/// 注文書を明細ごと削除する（DRAFT/SENTのみ呼び出し元でチェック済みの前提）
pub async fn delete_purchase_order(pool: &PgPool, order_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM t_purchase_order_item WHERE order_id = $1")
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM t_purchase_order WHERE order_id = $1")
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

// ── ダッシュボード進捗表示用（2026-07-12追加。P2-3: presentation/handlers/home.rs
//    直書きSQLのRepository層移行。締め日・ステータス計算等の表示ロジックは
//    home.rs側に残し、クエリのみここに移行）──

/// 発注進捗ダッシュボード用の行（PurchaseOrder + パートナー/案件/技術者/契約締め日）
#[derive(sqlx::FromRow)]
pub struct PurchaseOrderProgressRow {
    pub order_id: String,
    pub status: String,
    pub work_start: Option<chrono::NaiveDate>,
    pub work_end: Option<chrono::NaiveDate>,
    pub payment_condition: String,
    pub partner_name: String,
    pub project_id: String,
    pub project_name: String,
    pub engineer_name: String,
    /// 発注ヘッダのパートナー契約ID（未紐付け時はNULL）
    pub partner_contract_id: Option<i64>,
    // 契約(m_partner_contract)未設定の発注はLEFT JOINでNULLになりうるため全てOption
    pub order_create_deadline_day: Option<i32>,
    pub order_approve_deadline_days_before: Option<i32>,
    pub report_upload_deadline_days_before: Option<i32>,
    pub invoice_create_deadline_day: Option<i32>,
    pub invoice_approve_deadline_day: Option<i32>,
    // 稼働報告提出期限は案件(m_project)側の設定を優先する（受注進捗と一致させるため。
    // 2026-08-18: 発注進捗と受注進捗で別々の設定を参照しており、同じ稼働報告の期限が
    // 食い違って表示される不具合があったため統一）。NULLならcontract側にフォールバック。
    pub report_deadline_type: Option<String>,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
}

/// 受注進捗ダッシュボード用の行（ReceivedOrder + クライアント/技術者/契約締め日）
#[derive(sqlx::FromRow)]
pub struct ReceivedOrderProgressRow {
    pub id: i64,
    pub received_order_no: Option<String>,
    pub status: String,
    pub target_month: Option<chrono::NaiveDate>,
    pub work_end: Option<chrono::NaiveDate>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub payment_condition: String,
    pub client_name: String,
    pub engineer_name: String,
    /// 受注ヘッダのクライアント契約ID（未紐付け時はNULL）
    pub client_contract_id: Option<i64>,
    pub report_deadline_days_before: Option<i32>,
    /// 案件単位の提出期限設定（`m_project.report_deadline_*`）。未設定ならNoneのままフォールバック。
    pub report_deadline_type: Option<String>,
    pub report_deadline_value: Option<i32>,
    pub report_deadline_holiday_rule: Option<String>,
}

/// 発注進捗ダッシュボード用の一覧取得（CANCELLED除外）
///
/// PAID は「更新から3日超」かつ「作業開始月が直近6ヶ月より前」のときだけ除外する。
/// （月フィルタ付きプロジェクト詳細で、支払済みの過去月発注が消えないようにする）
pub async fn list_partner_progress_rows(pool: &PgPool) -> Result<Vec<PurchaseOrderProgressRow>> {
    let rows = sqlx::query_as::<_, PurchaseOrderProgressRow>(
        r#"
        SELECT po.order_id, po.status, po.work_start, po.work_end,
               po.payment_condition,
               p.name AS partner_name,
               po.project_id,
               pr.name AS project_name,
               COALESCE(e.name, item_e.name, '') AS engineer_name,
               COALESCE(po.partner_contract_id, item_e.partner_contract_id) AS partner_contract_id,
               pc.order_create_deadline_day,
               pc.order_approve_deadline_days_before,
               pc.report_upload_deadline_days_before,
               pc.invoice_create_deadline_day,
               pc.invoice_approve_deadline_day,
               pr.report_deadline_type,
               pr.report_deadline_value,
               pr.report_deadline_holiday_rule
        FROM t_purchase_order po
        JOIN m_partner p ON p.partner_id = po.partner_id
        LEFT JOIN m_project pr ON pr.project_id = po.project_id
        LEFT JOIN m_engineer e ON e.id = po.engineer_id
        LEFT JOIN m_partner_contract pc ON pc.id = po.partner_contract_id
        LEFT JOIN LATERAL (
            SELECT eng.name, poi.partner_contract_id
            FROM t_purchase_order_item poi
            JOIN m_partner_contract poi_pc ON poi_pc.id = poi.partner_contract_id
            JOIN m_engineer eng ON eng.id = poi_pc.engineer_id
            WHERE poi.purchase_order_pk = po.id
            ORDER BY poi.id
            LIMIT 1
        ) item_e ON true
        WHERE po.status != 'CANCELLED'
          AND NOT (
                po.status = 'PAID'
            AND po.updated_at < NOW() - INTERVAL '3 days'
            AND po.work_start < (date_trunc('month', CURRENT_DATE) - INTERVAL '5 months')::date
          )
        ORDER BY p.name, pr.name, po.work_start DESC
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 受注進捗ダッシュボード用の一覧取得（CANCELLED除外）
///
/// PAID 除外は発注と同様、直近6ヶ月より前の確定分のみ。
///
/// `project_id` は次の優先順で解決する（EDI取込の受注書は client_contract_id が空でも案件展開に出す）:
/// 1. 受注に紐づく受注契約 (`ro.client_contract_id`)
/// 2. 同一クライアント・技術者・対象月に重なる受注契約
/// 3. 受注の `project_name` + `client_id` で案件マスタ照合
pub async fn list_client_progress_rows(pool: &PgPool) -> Result<Vec<ReceivedOrderProgressRow>> {
    let rows = sqlx::query_as::<_, ReceivedOrderProgressRow>(
        r#"
        SELECT ro.id, ro.received_order_no, ro.status, ro.target_month,
               ro.work_end,
               COALESCE(cc.project_id, matched_cc.project_id, pr_by_name.project_id) AS project_id,
               ro.project_name, ro.payment_condition,
               c.name AS client_name,
               COALESCE(e.name, '') AS engineer_name,
               COALESCE(ro.client_contract_id, matched_cc.id) AS client_contract_id,
               COALESCE(cc.report_deadline_days_before, matched_cc.report_deadline_days_before) AS report_deadline_days_before,
               pr.report_deadline_type,
               pr.report_deadline_value,
               pr.report_deadline_holiday_rule
        FROM t_received_order ro
        JOIN m_client c ON c.id = ro.client_id
        LEFT JOIN m_engineer e ON e.id = ro.engineer_id
        LEFT JOIN m_client_contract cc ON cc.id = ro.client_contract_id
        LEFT JOIN LATERAL (
            SELECT cc2.id, cc2.project_id, cc2.report_deadline_days_before
            FROM m_client_contract cc2
            JOIN m_project p ON p.project_id = cc2.project_id AND p.client_id = ro.client_id
            WHERE ro.engineer_id IS NOT NULL
              AND cc2.engineer_id = ro.engineer_id
              AND cc2.start_date <= COALESCE(ro.target_month, ro.work_start)
              AND cc2.end_date >= COALESCE(ro.target_month, ro.work_start)
            ORDER BY cc2.is_active DESC, cc2.id DESC
            LIMIT 1
        ) matched_cc ON true
        LEFT JOIN m_project pr_by_name
          ON pr_by_name.client_id = ro.client_id
         AND NULLIF(BTRIM(ro.project_name), '') IS NOT NULL
         AND (
              BTRIM(pr_by_name.name) = BTRIM(ro.project_name)
              OR (
                  NULLIF(BTRIM(COALESCE(pr_by_name.edi_project_alias, '')), '') IS NOT NULL
                  AND BTRIM(pr_by_name.edi_project_alias) = BTRIM(ro.project_name)
              )
         )
        LEFT JOIN m_project pr
          ON pr.project_id = COALESCE(cc.project_id, matched_cc.project_id, pr_by_name.project_id)
        WHERE ro.status != 'CANCELLED'
          AND NOT (
                ro.status = 'PAID'
            AND ro.updated_at < NOW() - INTERVAL '3 days'
            AND COALESCE(ro.target_month, ro.work_start)
                < (date_trunc('month', CURRENT_DATE) - INTERVAL '5 months')::date
          )
        ORDER BY c.name, ro.target_month DESC
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── orders.rs 移行分（2026-07-13、P2-3続き）──

/// 発注書明細 + エンジニア名（表示用）
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct OrderItemWithEngineer {
    pub id: i64,
    pub order_id: String,
    pub partner_contract_id: i64,
    pub base_fee: i32,
    pub effort: Decimal,
    pub actual_hours: Decimal,
    pub settlement_type: String,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    /// DBカラム名はmigration 047で`price`→`amount`に変更したが、
    /// 公開JSON APIのキー名（フロントエンド契約）は変更しない方針のため`price`のまま維持する
    #[serde(rename = "price")]
    pub amount: i32,
    pub tax_rate: Decimal,
    pub engineer_name: String,
}

/// 発注書一覧表示用
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct OrderRow {
    pub order_id: String,
    pub partner_name: String,
    pub project_name: String,
    pub engineer_name: String,
    pub status: String,
    pub order_date: chrono::NaiveDate,
    pub work_start: chrono::NaiveDate,
    pub work_end: chrono::NaiveDate,
    pub item_count: i64,
    pub total_amount: Option<i64>,
}

/// 有効パートナー契約（発注書作成画面用）
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ActivePartnerContractRow {
    pub id: i64,
    pub partner_id: String,
    pub partner_name: String,
    pub project_id: String,
    pub project_name: String,
    pub engineer_id: i64,
    pub engineer_name: String,
    pub base_rate: i32,
    pub effort: Decimal,
    pub settlement_type: String,
}

/// 発注書作成時に選択された契約情報
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContractForOrder {
    pub id: i64,
    pub project_id: String,
    pub partner_id: String,
    pub engineer_id: i64,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub settlement_type: String,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub base_rate: i32,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub effort: Decimal,
    pub mid_month_rule: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub partner_name: String,
    pub project_name: String,
    pub engineer_name: String,
}

/// 発注書作成: 選択されたcontract_idsに対応する契約一覧を取得
pub async fn find_contracts_for_order(pool: &PgPool, contract_ids: &[i64]) -> Result<Vec<ContractForOrder>> {
    let placeholders: Vec<String> = (1..=contract_ids.len()).map(|i| format!("${}", i)).collect();
    let sql = format!(
        r#"
        SELECT pc.*, p.name AS partner_name, pr.name AS project_name,
               COALESCE(e.name, '') AS engineer_name
        FROM m_partner_contract pc
        JOIN m_partner p ON pc.partner_id = p.partner_id
        JOIN m_project pr ON pc.project_id = pr.project_id
        LEFT JOIN m_engineer e ON pc.engineer_id = e.id
        WHERE pc.id IN ({})
        "#,
        placeholders.join(", ")
    );
    let mut query = sqlx::query_as::<_, ContractForOrder>(&sql);
    for id in contract_ids {
        query = query.bind(id);
    }
    Ok(query.fetch_all(pool).await?)
}

/// 発注書番号採番用: 指定prefixパターンに一致する直近のorder_idを取得
pub async fn find_max_order_id_with_prefix(pool: &PgPool, prefix_pattern: &str) -> Result<Option<String>> {
    let max_seq: Option<String> = sqlx::query_scalar(
        "SELECT MAX(order_id) FROM t_purchase_order WHERE order_id LIKE $1"
    )
    .bind(prefix_pattern)
    .fetch_one(pool)
    .await?;
    Ok(max_seq)
}

/// 同一条件のDRAFT発注書が既に存在するか（二重作成防止）
pub async fn draft_order_exists<'e, E>(
    exec: E,
    partner_id: &str,
    project_id: &str,
    contract_id: i64,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
) -> Result<bool>
where
    E: sqlx::PgExecutor<'e>,
{
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_purchase_order WHERE partner_id = $1 AND project_id = $2 AND partner_contract_id = $3 AND work_start = $4 AND work_end = $5 AND status = 'DRAFT')"
    )
    .bind(partner_id)
    .bind(project_id)
    .bind(contract_id)
    .bind(work_start)
    .bind(work_end)
    .fetch_one(exec)
    .await?;
    Ok(exists)
}

/// 同一受注契約・同一対象月に未キャンセルの受注書が存在するか（重複作成防止）
pub async fn received_order_exists_for_contract_month(
    pool: &PgPool,
    client_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM t_received_order
            WHERE client_contract_id = $1
              AND target_month = $2
              AND status != 'CANCELLED'
        )
        "#,
    )
    .bind(client_contract_id)
    .bind(target_month)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// 発注書ヘッダーを新規作成する（簡易版: create()用）
pub async fn insert_purchase_order<'e, E>(
    exec: E,
    order_id: &str,
    partner_id: &str,
    project_id: &str,
    contract_id: i64,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        r#"
        INSERT INTO t_purchase_order (
            order_id, uuid, status, partner_id, project_id,
            partner_contract_id, order_date, work_start, work_end
        ) VALUES ($1, gen_random_uuid(), 'DRAFT', $2, $3, $4, CURRENT_DATE, $5, $6)
        "#
    )
    .bind(order_id)
    .bind(partner_id)
    .bind(project_id)
    .bind(contract_id)
    .bind(work_start)
    .bind(work_end)
    .execute(exec)
    .await?;
    Ok(())
}

/// 発注書ヘッダーを新規作成する（ロールフォワード用: 追加項目込み）
#[allow(clippy::too_many_arguments)]
pub async fn insert_purchase_order_full<'e, E>(
    exec: E,
    order_id: &str,
    partner_id: &str,
    project_id: &str,
    contract_id: i64,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
    workplace_id: Option<i64>,
    deliverable_text: &str,
    payment_condition: &str,
    contract_items: &str,
    remarks: &str,
) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        r#"
        INSERT INTO t_purchase_order (
            order_id, uuid, status, partner_id, project_id,
            partner_contract_id, order_date, work_start, work_end,
            workplace_id, deliverable_text, payment_condition,
            contract_items, remarks
        ) VALUES ($1, gen_random_uuid(), 'DRAFT', $2, $3, $4, CURRENT_DATE, $5, $6,
                  $7, $8, $9, $10, $11)
        "#
    )
    .bind(order_id)
    .bind(partner_id)
    .bind(project_id)
    .bind(contract_id)
    .bind(work_start)
    .bind(work_end)
    .bind(workplace_id)
    .bind(deliverable_text)
    .bind(payment_condition)
    .bind(contract_items)
    .bind(remarks)
    .execute(exec)
    .await?;
    Ok(())
}

/// 発注書明細を1件追加する — PgPool/トランザクション両対応
/// Phase C: purchase_order_pk (新 FK) も dual-write
#[allow(clippy::too_many_arguments)]
pub async fn insert_purchase_order_item<'e, E>(
    exec: E,
    order_id: &str,
    contract_id: i64,
    base_fee: i32,
    effort: Decimal,
    settlement_type: &str,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    fixed_hours: Option<Decimal>,
    deduction_rate: i32,
    overtime_rate: i32,
    price: i32,
) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query(
        r#"
        INSERT INTO t_purchase_order_item (
            order_id, partner_contract_id, base_fee, effort,
            actual_hours, settlement_type, lower_limit_hours,
            upper_limit_hours, fixed_hours, deduction_rate,
            overtime_rate, price, purchase_order_pk
        ) VALUES ($1, $2, $3, $4, 0, $5, $6, $7, $8, $9, $10, $11,
                  (SELECT id FROM t_purchase_order WHERE order_id = $1))
        "#
    )
    .bind(order_id)
    .bind(contract_id)
    .bind(base_fee)
    .bind(effort)
    .bind(settlement_type)
    .bind(lower_limit_hours)
    .bind(upper_limit_hours)
    .bind(fixed_hours)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(price)
    .execute(exec)
    .await?;
    Ok(())
}

/// 発注書明細一覧（ORDER BY id付き。PDF生成等の並び順が必要な箇所用）
pub async fn list_order_items_ordered(pool: &PgPool, order_id: &str) -> Result<Vec<PurchaseOrderItem>> {
    let rows = sqlx::query_as::<_, PurchaseOrderItem>(
        "SELECT * FROM t_purchase_order_item WHERE order_id = $1 ORDER BY id"
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 発注書明細一覧（エンジニア名結合、詳細API用）
pub async fn list_order_items_with_engineer(pool: &PgPool, order_id: &str) -> Result<Vec<OrderItemWithEngineer>> {
    let rows = sqlx::query_as::<_, OrderItemWithEngineer>(
        r#"SELECT poi.*,
               COALESCE(e.name, '(不明)') AS engineer_name
           FROM t_purchase_order_item poi
           LEFT JOIN m_partner_contract pc ON pc.id = poi.partner_contract_id
           LEFT JOIN m_engineer e ON e.id = pc.engineer_id
           WHERE poi.order_id = $1
           ORDER BY poi.id"#
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー名・メールアドレス取得
pub async fn find_partner_name_email(pool: &PgPool, partner_id: &str) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT name, COALESCE(email, '') FROM m_partner WHERE partner_id = $1"
    )
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナー名・メールアドレス・CC取得（メール送信プレビュー用）
pub async fn find_partner_name_email_cc(pool: &PgPool, partner_id: &str) -> Result<Option<(String, String, String)>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT name, COALESCE(email, ''), COALESCE(cc, '') FROM m_partner WHERE partner_id = $1"
    )
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナー名・住所・電話番号取得(PDF用)
pub async fn find_partner_name_address_tel(pool: &PgPool, partner_id: &str) -> Result<Option<(String, String, String)>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT name, COALESCE(address, ''), COALESCE(tel, '') FROM m_partner WHERE partner_id = $1"
    )
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナー代理請求書PDF用の発行者情報
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct PartnerInvoiceIssuerRow {
    pub name: String,
    pub postal_code: String,
    pub address: String,
    pub tel: String,
    pub representative: String,
    pub registration_no: String,
    pub bank_name: String,
    pub bank_branch: String,
    pub account_type: String,
    pub account_number: String,
    pub account_name: String,
}

/// パートナー代理請求書の発行者情報を取得する
pub async fn find_partner_invoice_issuer(pool: &PgPool, partner_id: &str) -> Result<Option<PartnerInvoiceIssuerRow>> {
    let row = sqlx::query_as::<_, PartnerInvoiceIssuerRow>(
        r#"SELECT name,
                  COALESCE(postal_code, '') AS postal_code,
                  COALESCE(address, '') AS address,
                  COALESCE(tel, '') AS tel,
                  COALESCE(representative_name, '') AS representative,
                  COALESCE(registration_no, '') AS registration_no,
                  COALESCE(bank_name, '') AS bank_name,
                  COALESCE(bank_branch, '') AS bank_branch,
                  COALESCE(account_type, '普通') AS account_type,
                  COALESCE(account_number, '') AS account_number,
                  COALESCE(account_name, '') AS account_name
           FROM m_partner WHERE partner_id = $1"#
    )
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 案件名取得
pub async fn find_project_name(pool: &PgPool, project_id: &str) -> Result<Option<String>> {
    let name: Option<String> = sqlx::query_scalar(
        "SELECT name FROM m_project WHERE project_id = $1"
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?;
    Ok(name)
}

/// 稼働報告 初回依頼メールを「そろそろ送るタイミングです」と社内へ通知する対象（発注書単位）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReportRequestNotifyTarget {
    pub order_id: String,
    pub partner_name: String,
    pub project_name: String,
    pub work_month: chrono::NaiveDate,
}

/// 案件の `report_request_day` が本日と一致する、SENT/ACCEPTED状態の発注書（当月分）を、
/// まだ通知していないものだけ返す（稼働報告 初回依頼の送り忘れ防止・社内通知用）。
pub async fn list_report_request_notify_targets(pool: &PgPool, today_day: i32) -> Result<Vec<ReportRequestNotifyTarget>> {
    let rows = sqlx::query_as::<_, ReportRequestNotifyTarget>(
        r#"SELECT po.order_id,
                  COALESCE(p.name, '') AS partner_name,
                  COALESCE(pr.name, '') AS project_name,
                  po.work_start AS work_month
           FROM t_purchase_order po
           JOIN m_project pr ON pr.project_id = po.project_id
           JOIN m_partner p ON p.partner_id = po.partner_id
           WHERE po.status IN ('SENT', 'ACCEPTED')
             AND po.report_request_notified_at IS NULL
             AND pr.report_request_day = $1
             AND DATE_TRUNC('month', po.work_start) = DATE_TRUNC('month', CURRENT_DATE)
           ORDER BY p.name, po.order_id"#
    )
    .bind(today_day)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 指定発注書に「稼働報告 初回依頼の社内通知済み」を記録する（二重通知防止）
pub async fn mark_report_request_notified(pool: &PgPool, order_id: &str) -> Result<()> {
    sqlx::query("UPDATE t_purchase_order SET report_request_notified_at = NOW() WHERE order_id = $1")
        .bind(order_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 発注注文書に紐づく案件名を取得する
pub async fn find_project_name_by_purchase_order(pool: &PgPool, order_id: &str) -> Result<Option<String>> {
    let name: Option<String> = sqlx::query_scalar(
        r#"SELECT p.name
           FROM t_purchase_order po
           JOIN m_project p ON p.project_id = po.project_id
           WHERE po.order_id = $1"#
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;
    Ok(name)
}

/// 自社情報（会社名・住所・電話・代表者名）取得
pub async fn find_company_info(pool: &PgPool) -> Result<Option<(String, String, String, String)>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT name, address, tel, representative_name FROM s_company_info ORDER BY id LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 発注契約: 技術者名 + 作業場所
pub async fn find_contract_engineer_and_workplace(pool: &PgPool, contract_id: i64) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT e.name, COALESCE(pc.work_location, '') FROM m_partner_contract pc JOIN m_engineer e ON e.id = pc.engineer_id WHERE pc.id = $1"
    )
    .bind(contract_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 発注契約: PDF用テンプレート情報（甲乙責任者・作業責任者・成果物・支払条件）
#[allow(clippy::type_complexity)]
pub async fn find_contract_pdf_template_info(pool: &PgPool, contract_id: i64) -> Result<Option<(String, String, String, String, String, String, String)>> {
    let row = sqlx::query_as(
        r#"SELECT COALESCE("甲_責任者", ''),
                  COALESCE("甲_担当者", ''),
                  COALESCE("乙_責任者", ''),
                  COALESCE("乙_担当者", ''),
                  COALESCE("作業責任者", ''),
                  COALESCE(deliverable_text, ''),
                  COALESCE(payment_condition, '')
           FROM m_partner_contract WHERE id = $1"#
    )
    .bind(contract_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 発注書ヘッダーの作業期間・作業場所を更新する
pub async fn update_purchase_order_work(
    pool: &PgPool,
    order_id: &str,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
    work_location: &str,
    payment_condition: Option<&str>,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE t_purchase_order
        SET work_start = $1,
            work_end = $2,
            work_location = $3,
            payment_condition = COALESCE($4, payment_condition),
            updated_at = NOW()
        WHERE order_id = $5
        "#
    )
    .bind(work_start)
    .bind(work_end)
    .bind(work_location)
    .bind(payment_condition)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 発注書明細の金額関連フィールドを更新する
#[allow(clippy::too_many_arguments)]
pub async fn update_purchase_order_item_fields(
    pool: &PgPool,
    item_id: i64,
    order_id: &str,
    base_fee: i32,
    effort: Decimal,
    settlement_type: &str,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    deduction_rate: i32,
    overtime_rate: i32,
    amount: i32,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE t_purchase_order_item SET
            base_fee = $1, effort = $2, settlement_type = $3,
            lower_limit_hours = $4, upper_limit_hours = $5,
            deduction_rate = $6, overtime_rate = $7, amount = $8
           WHERE id = $9 AND order_id = $10"#
    )
    .bind(base_fee)
    .bind(effort)
    .bind(settlement_type)
    .bind(lower_limit_hours)
    .bind(upper_limit_hours)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(amount)
    .bind(item_id)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 発注書のステータスを SENT に更新する（token_issued_at打刻込み。送付・訂正再送付共通）
pub async fn mark_purchase_order_sent(pool: &PgPool, order_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE t_purchase_order SET status = 'SENT', token_issued_at = NOW(), updated_at = NOW() WHERE order_id = $1"
    )
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 発注書一覧（パートナー/ステータスで絞り込み、JSON API用）
pub async fn list_orders_filtered(pool: &PgPool, partner: &str, status: &str) -> Result<Vec<OrderRow>> {
    let mut sql = String::from(
        r#"
        SELECT po.order_id, p.name AS partner_name, pr.name AS project_name,
               COALESCE(e.name, e2.name, '') AS engineer_name,
               po.status, po.order_date, po.work_start, po.work_end,
               COUNT(poi.id) AS item_count,
               SUM(poi.amount)::BIGINT AS total_amount
        FROM t_purchase_order po
        JOIN m_partner p ON po.partner_id = p.partner_id
        JOIN m_project pr ON po.project_id = pr.project_id
        LEFT JOIN m_engineer e ON e.id = po.engineer_id
        LEFT JOIN m_partner_contract pc ON pc.id = po.partner_contract_id
        LEFT JOIN m_engineer e2 ON e2.id = pc.engineer_id
        LEFT JOIN t_purchase_order_item poi ON poi.purchase_order_pk = po.id
        WHERE 1=1
        "#
    );

    let mut params: Vec<String> = Vec::new();
    if !partner.is_empty() {
        params.push(partner.to_string());
        sql.push_str(&format!(" AND po.partner_id = ${}", params.len()));
    }
    if !status.is_empty() {
        params.push(status.to_string());
        sql.push_str(&format!(" AND po.status = ${}", params.len()));
    }
    sql.push_str(" GROUP BY po.order_id, p.name, pr.name, e.name, e2.name ORDER BY po.order_date DESC LIMIT 100");

    let mut q = sqlx::query_as::<_, OrderRow>(&sql);
    for p in &params {
        q = q.bind(p);
    }

    Ok(q.fetch_all(pool).await?)
}

/// 発注契約フォーム用の有効契約一覧（発注書新規作成画面）
pub async fn list_active_contracts_for_order_form(pool: &PgPool) -> Result<Vec<ActivePartnerContractRow>> {
    let rows = sqlx::query_as::<_, ActivePartnerContractRow>(
        r#"
        SELECT pc.id, pc.partner_id, pc.project_id, pc.engineer_id,
               p.name AS partner_name, pr.name AS project_name, e.name AS engineer_name,
               pc.settlement_type, pc.base_rate, pc.effort
        FROM m_partner_contract pc
        JOIN m_partner p ON pc.partner_id = p.partner_id
        JOIN m_project pr ON pc.project_id = pr.project_id
        JOIN m_engineer e ON pc.engineer_id = e.id
        WHERE pc.is_active = true
        ORDER BY p.name, e.name
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 発注書明細を全件コピーする（ロールフォワード用、トランザクション内で使用）
pub async fn copy_order_items(
    conn: &mut sqlx::PgConnection,
    from_order_id: &str,
    to_order_id: &str,
) -> Result<Vec<PurchaseOrderItem>> {
    let items = sqlx::query_as::<_, PurchaseOrderItem>(
        "SELECT * FROM t_purchase_order_item WHERE order_id = $1"
    )
    .bind(from_order_id)
    .fetch_all(&mut *conn)
    .await?;

    for item in &items {
        insert_purchase_order_item(
            &mut *conn,
            to_order_id,
            item.partner_contract_id,
            item.base_fee,
            item.effort,
            &item.settlement_type,
            item.lower_limit_hours,
            item.upper_limit_hours,
            item.fixed_hours,
            item.deduction_rate,
            item.overtime_rate,
            item.amount,
        ).await?;
    }
    Ok(items)
}

// ── EDI-OASIS注文一覧・PDF生成（2026-07-13追加。P2-3続き:
//    presentation/handlers/home.rs 直書きSQLのRepository層移行）──

/// 指定対象年月で取込済みの受注書client_order_number一覧（EDI-OASIS取込済み判定用）
pub async fn find_imported_client_order_numbers(pool: &PgPool, target_month: &str) -> Result<Vec<String>> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT client_order_number FROM t_received_order WHERE target_month = $1 AND client_order_number IS NOT NULL AND client_order_number != ''"
    )
    .bind(target_month)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// order_pdfが未生成の注文書一覧（PDF一括生成バッチ用）
pub async fn list_purchase_orders_without_pdf(pool: &PgPool) -> Result<Vec<PurchaseOrder>> {
    let rows = sqlx::query_as::<_, PurchaseOrder>(
        "SELECT * FROM t_purchase_order WHERE COALESCE(order_pdf, '') = '' ORDER BY order_id"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 注文書明細（PDF出力用: 技術者名, 基本料金, 稼働率, 金額）
pub async fn find_order_items_for_pdf(pool: &PgPool, order_id: &str) -> Result<Vec<(String, i64, String, i64)>> {
    let rows: Vec<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT COALESCE(poi.engineer_name, CONCAT('契約#', poi.partner_contract_id)), \
         poi.base_fee::bigint, poi.effort::text, poi.amount::bigint \
         FROM t_purchase_order_item poi WHERE poi.order_id = $1 ORDER BY poi.id"
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 注文書PDFのDrive URLを保存する
pub async fn set_purchase_order_pdf(pool: &PgPool, order_id: &str, drive_url: &str) -> Result<()> {
    sqlx::query("UPDATE t_purchase_order SET order_pdf = $1 WHERE order_id = $2")
        .bind(drive_url)
        .bind(order_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── 顧客契約(m_client_contract) CRUD（2026-07-13追加。P2-3続き:
//    presentation/handlers/client_contracts.rs 直書きSQLのRepository層移行）──

/// 顧客契約詳細画面の受注明細一覧用
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct RecentOrderItem {
    pub order_id: i64,
    pub target_month: chrono::NaiveDate,
    pub engineer_name: String,
    pub unit_price: i32,
    pub man_month: Decimal,
    pub actual_hours: Decimal,
    pub adjustment: i32,
    pub amount: i32,
    pub status: String,
}

/// 有効な案件一覧（クライアント名JOIN済み）
pub async fn list_active_projects_with_client(pool: &PgPool) -> Result<Vec<ProjectWithClient>> {
    let rows = sqlx::query_as::<_, ProjectWithClient>(
        "SELECT p.*, c.name AS client_name FROM m_project p JOIN m_client c ON p.client_id = c.id WHERE p.is_active = true ORDER BY p.name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 有効なエンジニア一覧
pub async fn list_active_engineers(pool: &PgPool) -> Result<Vec<Engineer>> {
    let rows = sqlx::query_as::<_, Engineer>(
        "SELECT * FROM m_engineer WHERE is_active = true ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 顧客契約登録。作成された契約のidを返す
pub async fn insert_client_contract(pool: &PgPool, form: &ClientContractForm, is_active: bool) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO m_client_contract (
            project_id, engineer_id, start_date, end_date,
            settlement_type, base_rate, effort,
            lower_limit_hours, upper_limit_hours, fixed_hours,
            deduction_rate, overtime_rate,
            mid_month_rule, billing_timing, payment_terms,
            report_deadline_days_before, currency, remarks, is_active
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
        RETURNING id
        "#
    )
    .bind(&form.project_id)
    .bind(form.engineer_id)
    .bind(form.start_date)
    .bind(form.end_date)
    .bind(&form.settlement_type)
    .bind(form.base_rate)
    .bind(form.effort)
    .bind(form.lower_limit_hours)
    .bind(form.upper_limit_hours)
    .bind(form.fixed_hours)
    .bind(form.deduction_rate)
    .bind(form.overtime_rate)
    .bind(form.mid_month_rule.as_deref().unwrap_or("FULL_MONTH"))
    .bind(form.billing_timing.as_deref().unwrap_or("MONTHLY"))
    .bind(form.payment_terms.as_deref().unwrap_or(""))
    .bind(form.report_deadline_days_before.unwrap_or(5))
    .bind(form.currency.as_deref().unwrap_or("JPY"))
    .bind(form.remarks.as_deref().unwrap_or(""))
    .bind(is_active)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 顧客契約更新
/// 契約の終了日のみ延長する（承諾済み受注書によるロック対象外。
/// 過去に確定した精算条件・受注書には影響しないため、期間延長のみは常に許可する）
pub async fn extend_client_contract_end_date(pool: &PgPool, id: i64, new_end_date: chrono::NaiveDate) -> Result<()> {
    sqlx::query("UPDATE m_client_contract SET end_date = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_end_date)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_client_contract(pool: &PgPool, id: i64, form: &ClientContractForm, is_active: bool) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE m_client_contract SET
            project_id = $1, engineer_id = $2, start_date = $3, end_date = $4,
            settlement_type = $5, base_rate = $6, effort = $7,
            lower_limit_hours = $8, upper_limit_hours = $9, fixed_hours = $10,
            deduction_rate = $11, overtime_rate = $12,
            mid_month_rule = $13, billing_timing = $14, payment_terms = $15,
            report_deadline_days_before = $16, currency = $17, remarks = $18,
            is_active = $19, updated_at = NOW()
        WHERE id = $20
        "#
    )
    .bind(&form.project_id)
    .bind(form.engineer_id)
    .bind(form.start_date)
    .bind(form.end_date)
    .bind(&form.settlement_type)
    .bind(form.base_rate)
    .bind(form.effort)
    .bind(form.lower_limit_hours)
    .bind(form.upper_limit_hours)
    .bind(form.fixed_hours)
    .bind(form.deduction_rate)
    .bind(form.overtime_rate)
    .bind(form.mid_month_rule.as_deref().unwrap_or("FULL_MONTH"))
    .bind(form.billing_timing.as_deref().unwrap_or("MONTHLY"))
    .bind(form.payment_terms.as_deref().unwrap_or(""))
    .bind(form.report_deadline_days_before.unwrap_or(5))
    .bind(form.currency.as_deref().unwrap_or("JPY"))
    .bind(form.remarks.as_deref().unwrap_or(""))
    .bind(is_active)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 顧客契約一覧（案件名・クライアント名・エンジニア名JOIN済み、最大100件）
pub async fn list_client_contracts_with_names(pool: &PgPool) -> Result<Vec<ClientContractWithNames>> {
    let rows = sqlx::query_as::<_, ClientContractWithNames>(
        r#"
        SELECT cc.*, p.name AS project_name, c.name AS client_name, e.name AS engineer_name
        FROM m_client_contract cc
        JOIN m_project p ON cc.project_id = p.project_id
        JOIN m_client c ON p.client_id = c.id
        JOIN m_engineer e ON cc.engineer_id = e.id
        ORDER BY cc.id DESC LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 顧客契約詳細（案件名・クライアント名・エンジニア名JOIN済み）
pub async fn find_client_contract_with_names(pool: &PgPool, id: i64) -> Result<Option<ClientContractWithNames>> {
    let row = sqlx::query_as::<_, ClientContractWithNames>(
        r#"
        SELECT cc.*, p.name AS project_name, c.name AS client_name, e.name AS engineer_name
        FROM m_client_contract cc
        JOIN m_project p ON cc.project_id = p.project_id
        JOIN m_client c ON p.client_id = c.id
        JOIN m_engineer e ON cc.engineer_id = e.id
        WHERE cc.id = $1
        "#
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 顧客契約に紐づく直近の受注明細一覧（最大10件）
pub async fn list_recent_order_items_for_contract(pool: &PgPool, client_contract_id: i64) -> Result<Vec<RecentOrderItem>> {
    let rows = sqlx::query_as::<_, RecentOrderItem>(
        r#"
        SELECT roi.order_id, ro.target_month, roi.engineer_name,
               roi.unit_price, roi.man_month, roi.actual_hours,
               roi.adjustment, roi.amount, ro.status
        FROM t_received_order_item roi
        JOIN t_received_order ro ON roi.order_id = ro.id
        WHERE roi.client_contract_id = $1
        ORDER BY ro.target_month DESC LIMIT 10
        "#
    )
    .bind(client_contract_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 顧客契約がロック対象か（紐づく受注書がACCEPTED以降のステータスを持つか）
pub async fn count_locked_orders_for_client_contract(pool: &PgPool, client_contract_id: i64) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM t_received_order_item roi
        JOIN t_received_order ro ON roi.order_id = ro.id
        WHERE roi.client_contract_id = $1
          AND ro.status IN ('ACCEPTED', 'INVOICED', 'PAID')
        "#
    )
    .bind(client_contract_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// 顧客契約を削除する（更新件数を返す）
pub async fn delete_client_contract(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM m_client_contract WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ── エンジニア(m_engineer) 参照系（2026-07-13追加。P2-3続き:
//    presentation/handlers/daily_work_entry.rs 直書きSQLのRepository層移行）──

/// 指定エンジニアが指定パートナーに所属しているか
pub async fn engineer_belongs_to_partner(pool: &PgPool, engineer_id: i64, partner_id: &str) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM m_engineer WHERE id = $1 AND partner_id = $2)"
    )
    .bind(engineer_id)
    .bind(partner_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// エンジニアID・名前を取得
pub async fn find_engineer_id_name(pool: &PgPool, id: i64) -> Result<Option<(i64, String)>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM m_engineer WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナーに所属する有効なエンジニアのid・名前一覧
pub async fn list_engineer_id_name_by_partner(pool: &PgPool, partner_id: &str) -> Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM m_engineer WHERE partner_id = $1 AND is_active = true ORDER BY name"
    )
    .bind(partner_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 有効な全エンジニアのid・名前・メール一覧（招待モーダル用）
pub async fn list_active_engineer_options(pool: &PgPool) -> Result<Vec<(i64, String, Option<String>)>> {
    let rows: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, email FROM m_engineer WHERE is_active = true ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// エンジニアのメールアドレスを更新する
pub async fn update_engineer_email(pool: &PgPool, id: i64, email: &str) -> Result<()> {
    sqlx::query("UPDATE m_engineer SET email = $1 WHERE id = $2")
        .bind(email)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// エンジニア名・所属パートナーIDを取得
pub async fn find_engineer_name_partner_id(pool: &PgPool, id: i64) -> Result<Option<(String, Option<String>)>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT name, partner_id FROM m_engineer WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ── パートナー契約(m_partner_contract) CRUD（2026-07-13追加。P2-3続き:
//    presentation/handlers/partner_contracts.rs 直書きSQLのRepository層移行）──

/// パートナー契約フォーム入力（作成・更新共通）
pub struct PartnerContractFormInput<'a> {
    pub project_id: &'a str,
    pub engineer_id: i64,
    pub partner_id: &'a str,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub settlement_type: &'a str,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub base_rate: i32,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub effort: Decimal,
    pub mid_month_rule: &'a str,
    pub kou_responsible: &'a str,
    pub kou_contact: &'a str,
    pub otsu_responsible: &'a str,
    pub otsu_contact: &'a str,
    pub work_responsible: &'a str,
    pub deliverable_text: &'a str,
    pub payment_condition: &'a str,
    pub contract_items: &'a str,
    pub work_location: &'a str,
    pub remarks: &'a str,
    pub order_create_deadline_day: i32,
    pub order_approve_deadline_days_before: i32,
    pub report_upload_deadline_days_before: i32,
    pub invoice_create_deadline_day: i32,
    pub invoice_approve_deadline_day: i32,
}

/// パートナー契約登録。作成された契約のidを返す
pub async fn insert_partner_contract(pool: &PgPool, f: &PartnerContractFormInput<'_>) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO m_partner_contract (
            project_id, engineer_id, partner_id, start_date, end_date,
            settlement_type, lower_limit_hours, upper_limit_hours,
            fixed_hours, base_rate, deduction_rate, overtime_rate,
            effort, mid_month_rule,
            "甲_責任者", "甲_担当者", "乙_責任者", "乙_担当者", "作業責任者",
            deliverable_text, payment_condition, contract_items, work_location, remarks,
            order_create_deadline_day, order_approve_deadline_days_before,
            report_upload_deadline_days_before, invoice_create_deadline_day,
            invoice_approve_deadline_day,
            is_active
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                  $15, $16, $17, $18, $19, $20, $21, $22, $23, $24,
                  $25, $26, $27, $28, $29, true)
        RETURNING id
        "#
    )
    .bind(f.project_id).bind(f.engineer_id).bind(f.partner_id)
    .bind(f.start_date).bind(f.end_date).bind(f.settlement_type)
    .bind(f.lower_limit_hours).bind(f.upper_limit_hours).bind(f.fixed_hours)
    .bind(f.base_rate).bind(f.deduction_rate).bind(f.overtime_rate)
    .bind(f.effort).bind(f.mid_month_rule)
    .bind(f.kou_responsible).bind(f.kou_contact)
    .bind(f.otsu_responsible).bind(f.otsu_contact).bind(f.work_responsible)
    .bind(f.deliverable_text).bind(f.payment_condition)
    .bind(f.contract_items).bind(f.work_location).bind(f.remarks)
    .bind(f.order_create_deadline_day).bind(f.order_approve_deadline_days_before)
    .bind(f.report_upload_deadline_days_before).bind(f.invoice_create_deadline_day)
    .bind(f.invoice_approve_deadline_day)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// パートナー契約更新
/// 契約の終了日のみ延長する（承諾済み発注書によるロック対象外。
/// 過去に確定した精算条件・注文書には影響しないため、期間延長のみは常に許可する）
pub async fn extend_partner_contract_end_date(pool: &PgPool, id: i64, new_end_date: chrono::NaiveDate) -> Result<()> {
    sqlx::query("UPDATE m_partner_contract SET end_date = $1, updated_at = NOW() WHERE id = $2")
        .bind(new_end_date)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_partner_contract(pool: &PgPool, id: i64, f: &PartnerContractFormInput<'_>) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE m_partner_contract SET
            project_id = $1, engineer_id = $2, partner_id = $3,
            start_date = $4, end_date = $5, settlement_type = $6,
            lower_limit_hours = $7, upper_limit_hours = $8,
            fixed_hours = $9, base_rate = $10, deduction_rate = $11,
            overtime_rate = $12, effort = $13, mid_month_rule = $14,
            "甲_責任者" = $15, "甲_担当者" = $16,
            "乙_責任者" = $17, "乙_担当者" = $18,
            "作業責任者" = $19,
            deliverable_text = $20, payment_condition = $21,
            contract_items = $22, work_location = $23, remarks = $24,
            order_create_deadline_day = $25, order_approve_deadline_days_before = $26,
            report_upload_deadline_days_before = $27, invoice_create_deadline_day = $28,
            invoice_approve_deadline_day = $29,
            updated_at = NOW()
        WHERE id = $30
        "#
    )
    .bind(f.project_id).bind(f.engineer_id).bind(f.partner_id)
    .bind(f.start_date).bind(f.end_date).bind(f.settlement_type)
    .bind(f.lower_limit_hours).bind(f.upper_limit_hours).bind(f.fixed_hours)
    .bind(f.base_rate).bind(f.deduction_rate).bind(f.overtime_rate)
    .bind(f.effort).bind(f.mid_month_rule)
    .bind(f.kou_responsible).bind(f.kou_contact)
    .bind(f.otsu_responsible).bind(f.otsu_contact).bind(f.work_responsible)
    .bind(f.deliverable_text).bind(f.payment_condition)
    .bind(f.contract_items).bind(f.work_location).bind(f.remarks)
    .bind(f.order_create_deadline_day).bind(f.order_approve_deadline_days_before)
    .bind(f.report_upload_deadline_days_before).bind(f.invoice_create_deadline_day)
    .bind(f.invoice_approve_deadline_day)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// パートナー契約一覧表示用の行
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct PartnerContractRow {
    pub id: i64,
    pub partner_name: String,
    pub project_name: String,
    pub engineer_name: String,
    pub settlement_type: String,
    pub base_rate: i32,
    pub effort: Decimal,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub is_active: bool,
}

/// パートナー契約一覧（パートナー名・案件名・エンジニア名JOIN済み、最大100件）
pub async fn list_partner_contract_rows(pool: &PgPool) -> Result<Vec<PartnerContractRow>> {
    let rows = sqlx::query_as::<_, PartnerContractRow>(
        r#"
        SELECT pc.id, p.name AS partner_name, pr.name AS project_name,
               e.name AS engineer_name, pc.settlement_type, pc.base_rate,
               pc.effort, pc.start_date, pc.end_date, pc.is_active
        FROM m_partner_contract pc
        JOIN m_partner p ON pc.partner_id = p.partner_id
        JOIN m_project pr ON pc.project_id = pr.project_id
        JOIN m_engineer e ON pc.engineer_id = e.id
        ORDER BY pc.id DESC LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約ロック判定: 紐づく発注書がACCEPTED以降のステータスを持つか
pub async fn count_locked_orders_for_partner_contract(pool: &PgPool, contract_id: i64) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM t_purchase_order_item poi
        JOIN t_purchase_order po ON po.id = poi.purchase_order_pk
        WHERE poi.partner_contract_id = $1
          AND po.status IN ('ACCEPTED', 'REPORT_RECEIVED', 'NOTICE_CREATED', 'NOTICE_CONFIRMED', 'PAID')
        "#
    )
    .bind(contract_id)
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// パートナー契約詳細（JOIN済み全フィールド）
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct PartnerContractDetailRow {
    pub id: i64,
    pub partner_id: String,
    pub project_id: String,
    pub engineer_id: i64,
    pub partner_name: String,
    pub project_name: String,
    pub engineer_name: String,
    pub settlement_type: String,
    pub base_rate: i32,
    pub effort: Decimal,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub is_active: bool,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub mid_month_rule: String,
    pub kou_responsible: String,
    pub kou_contact: String,
    pub otsu_responsible: String,
    pub otsu_contact: String,
    pub work_responsible: String,
    pub deliverable_text: String,
    pub contract_items: String,
    pub work_location: String,
    pub payment_condition: String,
    pub remarks: String,
    pub order_create_deadline_day: i32,
    pub order_approve_deadline_days_before: i32,
    pub report_upload_deadline_days_before: i32,
    pub invoice_create_deadline_day: i32,
    pub invoice_approve_deadline_day: i32,
}

/// パートナー契約詳細を取得する
pub async fn find_partner_contract_detail(pool: &PgPool, id: i64) -> Result<Option<PartnerContractDetailRow>> {
    let row = sqlx::query_as::<_, PartnerContractDetailRow>(
        r#"
        SELECT pc.id, pc.partner_id, pc.project_id, pc.engineer_id,
               p.name AS partner_name, pr.name AS project_name, e.name AS engineer_name,
               COALESCE(pc.settlement_type, '') AS settlement_type, pc.base_rate, pc.effort,
               pc.start_date, pc.end_date, pc.is_active,
               pc.lower_limit_hours, pc.upper_limit_hours, pc.fixed_hours,
               pc.deduction_rate, pc.overtime_rate,
               COALESCE(pc.mid_month_rule, '') AS mid_month_rule,
               COALESCE(pc."甲_責任者", '') AS kou_responsible,
               COALESCE(pc."甲_担当者", '') AS kou_contact,
               COALESCE(pc."乙_責任者", '') AS otsu_responsible,
               COALESCE(pc."乙_担当者", '') AS otsu_contact,
               COALESCE(pc."作業責任者", '') AS work_responsible,
               COALESCE(pc.deliverable_text, '') AS deliverable_text,
               COALESCE(pc.contract_items, '') AS contract_items,
               COALESCE(pc.work_location, '') AS work_location,
               COALESCE(pc.payment_condition, '') AS payment_condition,
               COALESCE(pc.remarks, '') AS remarks,
               pc.order_create_deadline_day, pc.order_approve_deadline_days_before,
               pc.report_upload_deadline_days_before, pc.invoice_create_deadline_day,
               pc.invoice_approve_deadline_day
        FROM m_partner_contract pc
        JOIN m_partner p ON pc.partner_id = p.partner_id
        JOIN m_project pr ON pc.project_id = pr.project_id
        JOIN m_engineer e ON pc.engineer_id = e.id
        WHERE pc.id = $1
        "#
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナー契約に紐づく発注履歴（order_id, 開始, 終了, 基本料金, 稼働時間, 金額）
pub async fn list_order_history_for_partner_contract(pool: &PgPool, contract_id: i64) -> Result<Vec<(String, String, String, i32, String, i32)>> {
    let rows: Vec<(String, String, String, i32, String, i32)> = sqlx::query_as(
        r#"SELECT po.order_id,
               COALESCE(po.work_start::text, ''),
               COALESCE(po.work_end::text, ''),
               poi.base_fee,
               poi.actual_hours::text,
               poi.amount
           FROM t_purchase_order_item poi
           JOIN t_purchase_order po ON po.id = poi.purchase_order_pk
           WHERE poi.partner_contract_id = $1
           ORDER BY po.work_start DESC"#
    )
    .bind(contract_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約に紐づく支払通知履歴（notice_id, 対象月, 稼働時間, 金額）
pub async fn list_notice_history_for_partner_contract(pool: &PgPool, contract_id: i64) -> Result<Vec<(String, String, String, i32)>> {
    let rows: Vec<(String, String, String, i32)> = sqlx::query_as(
        r#"SELECT pn.notice_id,
               COALESCE(pn.target_month::text, ''),
               pni.actual_hours::text,
               pni.amount
           FROM t_payment_notice_item pni
           JOIN t_payment_notice pn ON pn.notice_id = pni.notice_id
           WHERE pni.partner_contract_id = $1
           ORDER BY pn.target_month DESC"#
    )
    .bind(contract_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約フォーム用マスタデータ: パートナー(id, name)一覧（name順）
pub async fn list_partner_id_name_options(pool: &PgPool) -> Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT partner_id, name FROM m_partner ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約フォーム用マスタデータ: 案件(id, name)一覧（name順）
pub async fn list_project_id_name_options(pool: &PgPool) -> Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT project_id, name FROM m_project ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約フォーム用マスタデータ: エンジニア(id, name)一覧（name順、全件）
pub async fn list_all_engineer_id_name_options(pool: &PgPool) -> Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM m_engineer ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約フォーム用マスタデータ: 作業場所(id, name)一覧（name順）
pub async fn list_work_location_id_name_options(pool: &PgPool) -> Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, name FROM m_work_location ORDER BY name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー契約を削除する（更新件数を返す）
pub async fn delete_partner_contract(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM m_partner_contract WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

// ── パートナーポータル（2026-07-13追加。P2-3続き:
//    presentation/handlers/partner_portal.rs 直書きSQLのRepository層移行）──

/// パートナーの注文書件数
pub async fn count_orders_for_partner(pool: &PgPool, partner_id: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM t_purchase_order WHERE partner_id = $1")
        .bind(partner_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// パートナーの稼働報告件数
pub async fn count_timesheets_for_partner(pool: &PgPool, partner_id: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM t_monthly_timesheet WHERE partner_id = $1")
        .bind(partner_id)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

/// パートナーポータル向け注文書一覧
#[allow(clippy::type_complexity)]
pub async fn list_portal_orders(pool: &PgPool, partner_id: &str) -> Result<Vec<(String, String, String, chrono::NaiveDate, chrono::NaiveDate, Option<i64>, String, uuid::Uuid)>> {
    let rows = sqlx::query_as(
        r#"SELECT
            po.order_id,
            COALESCE(pj.name, '') as project_name,
            COALESCE(e.name, '') as engineer_name,
            po.work_start,
            po.order_date,
            (SELECT COALESCE(SUM(poi.amount), 0) FROM t_purchase_order_item poi WHERE poi.purchase_order_pk = po.id) as total_amount,
            po.status,
            po.uuid
        FROM t_purchase_order po
        LEFT JOIN m_project pj ON pj.project_id = po.project_id
        LEFT JOIN m_engineer e ON e.id = po.engineer_id
        WHERE po.partner_id = $1
        ORDER BY po.work_start DESC, po.order_id DESC"#
    )
    .bind(partner_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー本人の注文書を検索する（他パートナーの注文書は取得できない）
pub async fn find_purchase_order_for_partner(pool: &PgPool, order_id: &str, partner_id: &str) -> Result<Option<PurchaseOrder>> {
    let row = sqlx::query_as::<_, PurchaseOrder>(
        "SELECT * FROM t_purchase_order WHERE order_id = $1 AND partner_id = $2"
    )
    .bind(order_id)
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 注文書をパートナーが承諾した際の確定処理（ステータス・ハッシュ・PDFパスを更新）
pub async fn finalize_order_acceptance(pool: &PgPool, order_id: &str, document_hash: &str, pdf_path: &str) -> Result<()> {
    sqlx::query(
        r#"UPDATE t_purchase_order
           SET status = 'ACCEPTED',
               partner_accepted_at = NOW(),
               document_hash = $1,
               acceptance_pdf = $2,
               updated_at = NOW()
           WHERE order_id = $3"#
    )
    .bind(document_hash)
    .bind(pdf_path)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// パートナーポータル向け請求書（支払通知）一覧
#[allow(clippy::type_complexity)]
pub async fn list_portal_notices(pool: &PgPool, partner_id: &str) -> Result<Vec<(String, String, chrono::NaiveDate, chrono::NaiveDate, i64, Option<chrono::DateTime<chrono::Utc>>, uuid::Uuid)>> {
    let rows = sqlx::query_as(
        r#"SELECT
            pn.notice_id,
            COALESCE(pj.name, '') as project_name,
            pn.target_month,
            pn.notice_date,
            pn.total,
            pn.partner_accepted_at,
            pn.uuid
        FROM t_payment_notice pn
        LEFT JOIN t_purchase_order po ON po.id = pn.purchase_order_pk
        LEFT JOIN m_project pj ON pj.project_id = po.project_id
        WHERE pn.partner_id = $1
        ORDER BY pn.target_month DESC, pn.notice_id DESC"#
    )
    .bind(partner_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// パートナー本人の支払通知を検索する
pub async fn find_payment_notice_for_partner(pool: &PgPool, notice_id: &str, partner_id: &str) -> Result<Option<PaymentNotice>> {
    let row = sqlx::query_as::<_, PaymentNotice>(
        "SELECT * FROM t_payment_notice WHERE notice_id = $1 AND partner_id = $2"
    )
    .bind(notice_id)
    .bind(partner_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 支払通知をパートナーが承諾した際の確定処理（confirmed_at・ハッシュを更新）
pub async fn confirm_payment_notice(pool: &PgPool, notice_id: &str, document_hash: &str) -> Result<()> {
    sqlx::query(
        "UPDATE t_payment_notice SET partner_accepted_at = NOW(), document_hash = $1, updated_at = NOW() WHERE notice_id = $2"
    )
    .bind(document_hash)
    .bind(notice_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 支払通知書のヘッダ情報を更新する（明細は対象外）
pub async fn update_payment_notice_header(
    pool: &PgPool,
    notice_id: &str,
    notice_date: chrono::NaiveDate,
    payment_due_date: Option<chrono::NaiveDate>,
    remarks: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE t_payment_notice SET notice_date = $1, payment_due_date = $2, remarks = $3, updated_at = NOW() WHERE notice_id = $4"
    )
    .bind(notice_date)
    .bind(payment_due_date)
    .bind(remarks)
    .bind(notice_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 支払通知書を削除する（明細を先に削除してからヘッダを削除）
pub async fn delete_payment_notice(pool: &PgPool, notice_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM t_payment_notice_item WHERE notice_id = $1")
        .bind(notice_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM t_payment_notice WHERE notice_id = $1")
        .bind(notice_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// 支払通知書メール送付日時を記録する
pub async fn mark_payment_notice_mail_sent(pool: &PgPool, notice_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE t_payment_notice SET mail_sent_at = NOW(), updated_at = NOW() WHERE notice_id = $1"
    )
    .bind(notice_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── パートナートークンアクセス（2026-07-13追加。P2-3続き:
//    presentation/handlers/token.rs 直書きSQLのRepository層移行）──

/// 支払通知書取得（UUID指定 — トークンアクセス用）
pub async fn find_payment_notice_by_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<Option<PaymentNotice>> {
    let row = sqlx::query_as::<_, PaymentNotice>(
        "SELECT * FROM t_payment_notice WHERE uuid = $1"
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// システム設定から自社名を取得する（未設定時はデフォルト値）
pub async fn find_company_name_setting(pool: &PgPool) -> Result<String> {
    let name: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(value, '有限会社マックプランニング') FROM s_system_setting WHERE key = 'COMPANY_NAME'"
    )
    .fetch_optional(pool)
    .await?;
    Ok(name.unwrap_or_else(|| "有限会社マックプランニング".into()))
}

/// トークンURL経由で注文書を承諾する（SENTのもののみ。更新件数を返す）
pub async fn accept_order_by_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_purchase_order SET status = 'ACCEPTED', partner_accepted_at = NOW(), updated_at = NOW() WHERE uuid = $1 AND status = 'SENT'"
    )
    .bind(uuid)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// トークンURL経由で支払通知書を承諾する（未承諾のもののみ。更新件数を返す）
pub async fn accept_notice_by_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_payment_notice SET partner_accepted_at = NOW(), updated_at = NOW() WHERE uuid = $1 AND partner_accepted_at IS NULL"
    )
    .bind(uuid)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 支払通知に紐づく注文書のステータスをNOTICE_CONFIRMEDに更新する（UUID指定）
pub async fn confirm_order_status_by_notice_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE t_purchase_order SET status = 'NOTICE_CONFIRMED', updated_at = NOW()
        WHERE order_id = (SELECT purchase_order_id FROM t_payment_notice WHERE uuid = $1)
        "#
    )
    .bind(uuid)
    .execute(pool)
    .await?;
    Ok(())
}

/// 注文書ID・パートナー名・パートナーメール・案件名・作業期間をUUID指定で取得する（承諾通知メール用）
pub async fn find_order_id_partner_info_by_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<Option<(String, String, String, String, chrono::NaiveDate, chrono::NaiveDate)>> {
    let row = sqlx::query_as(
        r#"SELECT po.order_id, COALESCE(p.name, ''), COALESCE(p.email, ''),
                  COALESCE(pr.name, ''), po.work_start, po.work_end
           FROM t_purchase_order po
           LEFT JOIN m_partner p ON p.partner_id = po.partner_id
           LEFT JOIN m_project pr ON pr.project_id = po.project_id
           WHERE po.uuid = $1"#
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 支払通知ID・パートナー名・対象年月・税込合計をUUID指定で取得する（承諾通知メール用）
pub async fn find_notice_id_partner_name_by_uuid(pool: &PgPool, uuid: &uuid::Uuid) -> Result<Option<(String, String, chrono::NaiveDate, i32)>> {
    let row = sqlx::query_as(
        r#"SELECT pn.notice_id, COALESCE(p.name, ''), pn.target_month, pn.total
           FROM t_payment_notice pn
           LEFT JOIN t_purchase_order po ON po.id = pn.purchase_order_pk
           LEFT JOIN m_partner p ON p.partner_id = po.partner_id
           WHERE pn.uuid = $1"#
    )
    .bind(uuid)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ── 受注書(t_received_order) CRUD（2026-07-13追加。P2-3続き:
//    presentation/handlers/received_orders.rs 直書きSQLのRepository層移行）──

/// 受注契約(m_client_contract) + 案件/顧客/技術者情報（受注書作成用）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ClientContractForOrder {
    pub id: i64,
    pub project_id: String,
    pub engineer_id: i64,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub settlement_type: String,
    pub lower_limit_hours: Decimal,
    pub upper_limit_hours: Decimal,
    pub fixed_hours: Option<Decimal>,
    pub base_rate: i32,
    pub deduction_rate: i32,
    pub overtime_rate: i32,
    pub effort: Decimal,
    pub mid_month_rule: String,
    pub billing_timing: String,
    pub payment_terms: String,
    pub report_deadline_days_before: i32,
    pub currency: String,
    pub remarks: String,
    pub is_active: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub project_name: String,
    pub client_id: i64,
    pub engineer_name: String,
}

/// 受注書作成: 選択されたcontract_idsに対応する受注契約一覧を取得
pub async fn find_client_contracts_for_order(pool: &PgPool, contract_ids: &[i64]) -> Result<Vec<ClientContractForOrder>> {
    let placeholders: Vec<String> = (1..=contract_ids.len()).map(|i| format!("${}", i)).collect();
    let sql = format!(
        r#"
        SELECT cc.*, p.name AS project_name, p.client_id, e.name AS engineer_name
        FROM m_client_contract cc
        JOIN m_project p ON cc.project_id = p.project_id
        JOIN m_engineer e ON cc.engineer_id = e.id
        WHERE cc.id IN ({})
        "#,
        placeholders.join(", ")
    );
    let mut query = sqlx::query_as::<_, ClientContractForOrder>(&sql);
    for id in contract_ids {
        query = query.bind(id);
    }
    Ok(query.fetch_all(pool).await?)
}

/// 受注書作成: 単一の受注契約IDから取得（JSON API用）
pub async fn find_client_contract_for_order(pool: &PgPool, contract_id: i64) -> Result<Option<ClientContractForOrder>> {
    let row = sqlx::query_as::<_, ClientContractForOrder>(
        r#"
        SELECT cc.*, p.name AS project_name, p.client_id, e.name AS engineer_name
        FROM m_client_contract cc
        JOIN m_project p ON cc.project_id = p.project_id
        JOIN m_engineer e ON cc.engineer_id = e.id
        WHERE cc.id = $1
        "#
    )
    .bind(contract_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 受注書番号採番用: 指定月のprefixに一致する既存件数を取得（トランザクション内）
pub async fn count_received_orders_with_prefix(tx: &mut sqlx::PgConnection, prefix_pattern: &str) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_received_order WHERE received_order_no LIKE $1"
    )
    .bind(prefix_pattern)
    .fetch_one(&mut *tx)
    .await?;
    Ok(count)
}

/// 受注書を作成する（engineer_id / client_contract_id 未設定。複数契約一括作成フロー用）
pub async fn insert_received_order_basic(
    tx: &mut sqlx::PgConnection,
    received_order_no: &str,
    client_id: i64,
    target_month: chrono::NaiveDate,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
    project_name: &str,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            received_order_no, client_id, target_month, work_start, work_end,
            project_name, status, order_date
        ) VALUES ($1, $2, $3, $4, $5, $6, 'REGISTERED', CURRENT_DATE)
        RETURNING id
        "#
    )
    .bind(received_order_no)
    .bind(client_id)
    .bind(target_month)
    .bind(work_start)
    .bind(work_end)
    .bind(project_name)
    .fetch_one(&mut *tx)
    .await?;
    Ok(id)
}

/// 受注書を作成する（engineer_id / client_contract_id 設定あり。JSON API用）
#[allow(clippy::too_many_arguments)]
pub async fn insert_received_order_with_contract(
    tx: &mut sqlx::PgConnection,
    received_order_no: &str,
    client_id: i64,
    engineer_id: i64,
    client_contract_id: i64,
    target_month: chrono::NaiveDate,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
    project_name: &str,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            received_order_no, client_id, engineer_id, client_contract_id,
            target_month, work_start, work_end,
            project_name, status, order_date
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'REGISTERED', CURRENT_DATE)
        RETURNING id
        "#
    )
    .bind(received_order_no)
    .bind(client_id)
    .bind(engineer_id)
    .bind(client_contract_id)
    .bind(target_month)
    .bind(work_start)
    .bind(work_end)
    .bind(project_name)
    .fetch_one(&mut *tx)
    .await?;
    Ok(id)
}

/// 受注明細を1件作成する（契約の精算条件をスナップショット保存）
#[allow(clippy::too_many_arguments)]
pub async fn insert_received_order_item(
    tx: &mut sqlx::PgConnection,
    order_id: i64,
    client_contract_id: i64,
    engineer_name: &str,
    base_rate: i32,
    effort: Decimal,
    settlement_type: &str,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    fixed_hours: Option<Decimal>,
    deduction_rate: i32,
    overtime_rate: i32,
    mid_month_rule: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_received_order_item (
            order_id, client_contract_id, engineer_name, unit_price,
            man_month, settlement_type, base_rate,
            lower_limit_hours, upper_limit_hours, fixed_hours,
            deduction_rate, overtime_rate, effort, mid_month_rule,
            amount
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                  ($4::NUMERIC * $5)::INTEGER)
        "#
    )
    .bind(order_id)
    .bind(client_contract_id)
    .bind(engineer_name)
    .bind(base_rate)
    .bind(effort)
    .bind(settlement_type)
    .bind(base_rate)
    .bind(lower_limit_hours)
    .bind(upper_limit_hours)
    .bind(fixed_hours)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(effort)
    .bind(mid_month_rule)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// 受注明細を1件作成する（Peppol Order受信等、契約未紐付けの明細用。
/// 精算条件はDEFAULT値のまま作成し、後で担当者が手動リンク・調整する想定）
pub async fn insert_received_order_item_minimal(
    tx: &mut sqlx::PgConnection,
    order_id: i64,
    engineer_name: &str,
    unit_price: i32,
    man_month: Decimal,
    actual_hours: Decimal,
    amount: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_received_order_item (
            order_id, engineer_name, unit_price, man_month, actual_hours, amount
        ) VALUES ($1, $2, $3, $4, $5, $6)
        "#
    )
    .bind(order_id)
    .bind(engineer_name)
    .bind(unit_price)
    .bind(man_month)
    .bind(actual_hours)
    .bind(amount)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// 受注書のステータスを更新する（idで検索）
pub async fn update_received_order_status_by_id(pool: &PgPool, id: i64, status: &str) -> Result<()> {
    sqlx::query("UPDATE t_received_order SET status = $1, updated_at = NOW() WHERE id = $2")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 受注書を取得する
pub async fn find_received_order(pool: &PgPool, id: i64) -> Result<Option<ReceivedOrder>> {
    let row = sqlx::query_as::<_, ReceivedOrder>(
        "SELECT * FROM t_received_order WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 受注書に紐付け可能な受注契約候補（同一クライアント・技術者・対象月期間）
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ContractLinkCandidate {
    pub id: i64,
    pub engineer_name: String,
    pub project_id: String,
    pub project_name: String,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub is_active: bool,
    /// 案件名または EDI別名が受注書の project_name と一致
    pub name_matched: bool,
}

pub async fn list_contract_link_candidates(
    pool: &PgPool,
    received_order_id: i64,
) -> Result<Vec<ContractLinkCandidate>> {
    let rows = sqlx::query_as::<_, ContractLinkCandidate>(
        r#"
        SELECT cc.id,
               e.name AS engineer_name,
               pr.project_id,
               pr.name AS project_name,
               cc.start_date,
               cc.end_date,
               cc.is_active,
               (
                 pr.name = ro.project_name
                 OR (
                   NULLIF(BTRIM(COALESCE(pr.edi_project_alias, '')), '') IS NOT NULL
                   AND BTRIM(pr.edi_project_alias) = BTRIM(ro.project_name)
                 )
               ) AS name_matched
          FROM t_received_order ro
          JOIN m_client_contract cc ON true
          JOIN m_project pr ON pr.project_id = cc.project_id AND pr.client_id = ro.client_id
          JOIN m_engineer e ON e.id = cc.engineer_id
         WHERE ro.id = $1
           AND (ro.engineer_id IS NULL OR cc.engineer_id = ro.engineer_id)
           AND cc.start_date <= COALESCE(ro.target_month, ro.work_start)
           AND cc.end_date >= COALESCE(ro.target_month, ro.work_start)
         ORDER BY name_matched DESC, cc.is_active DESC, pr.name, e.name, cc.id
        "#,
    )
    .bind(received_order_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// EDI/PDF取込後の受注契約自動紐付け結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoLinkOutcome {
    Linked { contract_id: i64 },
    AlreadyLinked { contract_id: i64 },
    NoMatch,
    Ambiguous { count: i64 },
}

/// 取込直後に、案件名/EDI別名 + 技術者 + 対象月で一意に契約を特定できれば紐付ける。
/// 候補0件・複数件のときは紐付けず、オペレーター手動紐付けに委ねる。
pub async fn try_auto_link_received_order_contract(
    pool: &PgPool,
    received_order_id: i64,
) -> Result<AutoLinkOutcome> {
    let existing: Option<Option<i64>> = sqlx::query_scalar(
        "SELECT client_contract_id FROM t_received_order WHERE id = $1",
    )
    .bind(received_order_id)
    .fetch_optional(pool)
    .await?;

    match existing {
        None => anyhow::bail!("受注書が見つかりません: id={received_order_id}"),
        Some(Some(contract_id)) => {
            return Ok(AutoLinkOutcome::AlreadyLinked { contract_id });
        }
        Some(None) => {}
    }

    let ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT cc.id
          FROM t_received_order ro
          JOIN m_client_contract cc ON true
          JOIN m_project pr ON pr.project_id = cc.project_id AND pr.client_id = ro.client_id
         WHERE ro.id = $1
           AND (ro.engineer_id IS NULL OR cc.engineer_id = ro.engineer_id)
           AND cc.start_date <= COALESCE(ro.target_month, ro.work_start)
           AND cc.end_date   >= COALESCE(ro.target_month, ro.work_start)
           AND (
                pr.name = ro.project_name
             OR (
                  NULLIF(BTRIM(COALESCE(pr.edi_project_alias, '')), '') IS NOT NULL
                  AND BTRIM(pr.edi_project_alias) = BTRIM(ro.project_name)
                )
           )
         ORDER BY cc.is_active DESC, cc.id
        "#,
    )
    .bind(received_order_id)
    .fetch_all(pool)
    .await?;

    match ids.as_slice() {
        [] => Ok(AutoLinkOutcome::NoMatch),
        [contract_id] => {
            link_received_order_to_contract(pool, received_order_id, *contract_id).await?;
            Ok(AutoLinkOutcome::Linked {
                contract_id: *contract_id,
            })
        }
        many => Ok(AutoLinkOutcome::Ambiguous {
            count: many.len() as i64,
        }),
    }
}

pub fn log_auto_link_outcome(context: &str, order_id: i64, outcome: &AutoLinkOutcome) {
    match outcome {
        AutoLinkOutcome::Linked { contract_id } => {
            tracing::info!(
                "[{context}] 受注契約を自動紐付け: order_id={order_id}, contract_id={contract_id}"
            );
        }
        AutoLinkOutcome::AlreadyLinked { .. } => {}
        AutoLinkOutcome::NoMatch => {
            tracing::warn!(
                "[{context}] 受注契約を自動紐付けできません（候補なし）: order_id={order_id}。受注書詳細でオペレーターが手動紐付けしてください"
            );
        }
        AutoLinkOutcome::Ambiguous { count } => {
            tracing::warn!(
                "[{context}] 受注契約を自動紐付けできません（候補{count}件で曖昧）: order_id={order_id}。受注書詳細でオペレーターが手動紐付けしてください"
            );
        }
    }
}

/// 受注書ヘッダ／明細に受注契約を紐付ける（オペレーター手動 / 自動）
pub async fn link_received_order_to_contract(
    pool: &PgPool,
    received_order_id: i64,
    client_contract_id: i64,
) -> Result<()> {
    let mut tx = pool.begin().await?;

    let ok: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
              FROM t_received_order ro
              JOIN m_client_contract cc ON cc.id = $2
              JOIN m_project pr ON pr.project_id = cc.project_id
             WHERE ro.id = $1
               AND pr.client_id = ro.client_id
               AND (ro.engineer_id IS NULL OR cc.engineer_id = ro.engineer_id)
               AND cc.start_date <= COALESCE(ro.target_month, ro.work_start)
               AND cc.end_date >= COALESCE(ro.target_month, ro.work_start)
        )
        "#,
    )
    .bind(received_order_id)
    .bind(client_contract_id)
    .fetch_one(&mut *tx)
    .await?;

    if !ok {
        anyhow::bail!("選択した受注契約はこの受注書に紐付けできません（クライアント・技術者・期間を確認）");
    }

    sqlx::query(
        r#"
        UPDATE t_received_order ro
           SET client_contract_id = $2,
               engineer_id = COALESCE(
                 ro.engineer_id,
                 (SELECT cc.engineer_id FROM m_client_contract cc WHERE cc.id = $2)
               ),
               updated_at = NOW()
         WHERE ro.id = $1
        "#,
    )
    .bind(received_order_id)
    .bind(client_contract_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        UPDATE t_received_order_item
           SET client_contract_id = $2
         WHERE order_id = $1
           AND (client_contract_id IS NULL OR client_contract_id <> $2)
        "#,
    )
    .bind(received_order_id)
    .bind(client_contract_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// 顧客名・メールアドレスを取得する（報告書メール送付用）
pub async fn find_client_name_email(pool: &PgPool, client_id: i64) -> Result<Option<(String, String)>> {
    let row = sqlx::query_as(
        "SELECT name, COALESCE(email, '') FROM m_client WHERE id = $1"
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 受注書を報告書送付済みにする
pub async fn mark_received_order_report_sent(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_received_order SET status = 'REPORT_SENT', report_sent_to_client = true, report_sent_at = NOW(), updated_at = NOW() WHERE id = $1"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 一覧表示用の受注行（JOIN結果）
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ReceivedOrderRow {
    pub id: i64,
    pub received_order_no: String,
    pub target_month: chrono::NaiveDate,
    pub project_name: String,
    pub engineer_name: String,
    pub status: String,
    pub client_name: String,
    pub is_recurring: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub item_count: i64,
    pub total_amount: Option<i64>,
}

/// 受注書一覧（顧客名JOIN済み、最大100件）
pub async fn list_received_order_rows(pool: &PgPool) -> Result<Vec<ReceivedOrderRow>> {
    let rows = sqlx::query_as::<_, ReceivedOrderRow>(
        r#"
        SELECT ro.id, COALESCE(ro.received_order_no, '') AS received_order_no,
               ro.target_month, COALESCE(ro.project_name, '') AS project_name,
               COALESCE(
                 (SELECT roi2.engineer_name
                  FROM t_received_order_item roi2 WHERE roi2.order_id = ro.id LIMIT 1),
                 ''
               ) AS engineer_name,
               ro.status, c.name AS client_name, ro.is_recurring, ro.created_at,
               COUNT(roi.id)::BIGINT AS item_count,
               SUM(roi.amount)::BIGINT AS total_amount
        FROM t_received_order ro
        JOIN m_client c ON c.id = ro.client_id
        LEFT JOIN t_received_order_item roi ON roi.order_id = ro.id
        GROUP BY ro.id, c.name
        ORDER BY ro.target_month DESC, ro.id DESC
        LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 受注明細一覧を取得する
pub async fn list_received_order_items(pool: &PgPool, order_id: i64) -> Result<Vec<ReceivedOrderItem>> {
    let rows = sqlx::query_as::<_, ReceivedOrderItem>(
        "SELECT * FROM t_received_order_item WHERE order_id = $1 ORDER BY id"
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 顧客名を取得する
pub async fn find_client_name(pool: &PgPool, client_id: i64) -> Result<String> {
    let name: String = sqlx::query_scalar("SELECT name FROM m_client WHERE id = $1")
        .bind(client_id)
        .fetch_one(pool)
        .await?;
    Ok(name)
}

/// 技術者名を ID で取得する（WebAPI 受注登録用）
pub async fn find_engineer_name_by_id(pool: &PgPool, engineer_id: i64) -> Result<Option<String>> {
    let name: Option<String> = sqlx::query_scalar("SELECT name FROM m_engineer WHERE id = $1")
        .bind(engineer_id)
        .fetch_optional(pool)
        .await?;
    Ok(name)
}

/// 受注書の備考を更新する
pub async fn update_received_order_remarks(
    tx: &mut sqlx::PgConnection,
    order_id: i64,
    remarks: &str,
) -> Result<()> {
    sqlx::query("UPDATE t_received_order SET remarks = $1, updated_at = NOW() WHERE id = $2")
        .bind(remarks)
        .bind(order_id)
        .execute(&mut *tx)
        .await?;
    Ok(())
}

/// 受注書ヘッダーを更新する（編集画面用）
#[allow(clippy::too_many_arguments)]
pub async fn update_received_order_header(
    pool: &PgPool,
    id: i64,
    target_month: chrono::NaiveDate,
    work_start: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
    order_date: chrono::NaiveDate,
    project_name: &str,
    client_order_number: &str,
    remarks: &str,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE t_received_order SET
            target_month = $1, work_start = $2, work_end = $3,
            order_date = $4,
            project_name = $5, client_order_number = $6, remarks = $7,
            updated_at = NOW()
           WHERE id = $8"#
    )
    .bind(target_month).bind(work_start).bind(work_end)
    .bind(order_date)
    .bind(project_name)
    .bind(client_order_number)
    .bind(remarks)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 受注明細を更新する（編集画面用）
pub async fn update_received_order_item_fields(
    pool: &PgPool,
    item_id: i64,
    order_id: i64,
    unit_price: i32,
    man_month: Decimal,
    actual_hours: Decimal,
    adjustment: i32,
    amount: i32,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE t_received_order_item SET
            unit_price = $1, man_month = $2, actual_hours = $3,
            adjustment = $4, amount = $5
           WHERE id = $6 AND order_id = $7"#
    )
    .bind(unit_price).bind(man_month).bind(actual_hours)
    .bind(adjustment).bind(amount)
    .bind(item_id).bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 受注書を明細ごと削除する
pub async fn delete_received_order(pool: &PgPool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM t_received_order_item WHERE order_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM t_received_order WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

// ── 支払通知書(t_payment_notice) CRUD（2026-07-13追加。P2-3続き:
//    presentation/handlers/notices.rs 直書きSQLのRepository層移行）──

/// 発注注文書のヘッダー情報（パートナーID・案件ID・作業開始日）を取得する（支払通知書作成用）
pub async fn find_purchase_order_for_notice(pool: &PgPool, order_id: &str) -> Result<Option<(String, String, chrono::NaiveDate)>> {
    let row = sqlx::query_as(
        "SELECT partner_id, project_id, work_start FROM t_purchase_order WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 発注注文書（なければ紐づくパートナー契約）の支払条件テキストを取得する
pub async fn find_purchase_order_payment_condition(pool: &PgPool, order_id: &str) -> Result<Option<String>> {
    let cond: Option<String> = sqlx::query_scalar(
        r#"
        SELECT COALESCE(
            NULLIF(TRIM(po.payment_condition), ''),
            NULLIF(TRIM(pc.payment_condition), ''),
            NULLIF(TRIM(poi_pc.payment_condition), ''),
            ''
        )
          FROM t_purchase_order po
          LEFT JOIN m_partner_contract pc ON pc.id = po.partner_contract_id
          LEFT JOIN LATERAL (
              SELECT poi_pc.payment_condition
                FROM t_purchase_order_item poi
                JOIN m_partner_contract poi_pc ON poi_pc.id = poi.partner_contract_id
               WHERE poi.purchase_order_pk = po.id
               ORDER BY poi.id
               LIMIT 1
          ) poi_pc ON true
         WHERE po.order_id = $1
        "#
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;
    Ok(cond)
}

/// 支払通知書番号採番用: 指定prefixに一致する直近のnotice_idを取得
pub async fn find_max_notice_id_with_prefix(pool: &PgPool, prefix_pattern: &str) -> Result<Option<String>> {
    let max_seq: Option<String> = sqlx::query_scalar(
        "SELECT MAX(notice_id) FROM t_payment_notice WHERE notice_id LIKE $1"
    )
    .bind(prefix_pattern)
    .fetch_one(pool)
    .await?;
    Ok(max_seq)
}

/// パートナー契約＋対象月に紐づく発注注文書IDを取得する（月次確定の支払通知発行用）。
///
/// ヘッダの `partner_contract_id` または明細側の契約一致を許容し、
/// 発行可能なステータス（ACCEPTED / REPORT_RECEIVED）のみ対象とする。
pub async fn find_purchase_order_id_for_contract_month(
    pool: &PgPool,
    partner_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<Option<String>> {
    let order_id: Option<String> = sqlx::query_scalar(
        r#"
        SELECT po.order_id
          FROM t_purchase_order po
         WHERE DATE_TRUNC('month', po.work_start)::date = $2
           AND po.status IN ('ACCEPTED', 'REPORT_RECEIVED')
           AND (
               po.partner_contract_id = $1
               OR EXISTS (
                   SELECT 1 FROM t_purchase_order_item poi
                    WHERE poi.purchase_order_pk = po.id
                      AND poi.partner_contract_id = $1
               )
           )
         ORDER BY CASE po.status
                    WHEN 'REPORT_RECEIVED' THEN 0
                    WHEN 'ACCEPTED' THEN 1
                    ELSE 2
                  END,
                  po.order_id
         LIMIT 1
        "#,
    )
    .bind(partner_contract_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await?;
    Ok(order_id)
}

/// パートナー契約＋対象月に紐づく発注注文書（ステータス問わず1件）を診断用に取得する。
pub async fn find_purchase_order_status_for_contract_month(
    pool: &PgPool,
    partner_contract_id: i64,
    target_month: chrono::NaiveDate,
) -> Result<Option<(String, String)>> {
    let row: Option<(String, String)> = sqlx::query_as(
        r#"
        SELECT po.order_id, po.status
          FROM t_purchase_order po
         WHERE DATE_TRUNC('month', po.work_start)::date = $2
           AND (
               po.partner_contract_id = $1
               OR EXISTS (
                   SELECT 1 FROM t_purchase_order_item poi
                    WHERE poi.purchase_order_pk = po.id
                      AND poi.partner_contract_id = $1
               )
           )
         ORDER BY po.updated_at DESC, po.order_id DESC
         LIMIT 1
        "#,
    )
    .bind(partner_contract_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 支払通知書を作成する
#[allow(clippy::too_many_arguments)]
pub async fn insert_payment_notice(
    pool: &PgPool,
    notice_id: &str,
    purchase_order_id: &str,
    partner_id: &str,
    target_month: chrono::NaiveDate,
    payment_due_date: Option<chrono::NaiveDate>,
    subtotal: i32,
    tax_amount: i32,
    total: i32,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_payment_notice (
            notice_id, uuid, purchase_order_id, partner_id, target_month,
            notice_date, payment_due_date, subtotal, tax_amount, total,
            purchase_order_pk
        ) VALUES ($1, gen_random_uuid(), $2, $3, $4, CURRENT_DATE, $5, $6, $7, $8,
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
    .execute(pool)
    .await?;
    Ok(())
}

/// 支払通知書明細を1件作成する（発注明細からコピー）
#[allow(clippy::too_many_arguments)]
pub async fn insert_payment_notice_item(
    pool: &PgPool,
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
    amount: i32,
    tax_rate: Decimal,
) -> Result<()> {
    let adjustment = amount - base_fee;
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
    .execute(pool)
    .await?;
    Ok(())
}

/// 発注注文書のステータスを NOTICE_CREATED に更新する
pub async fn mark_purchase_order_notice_created(pool: &PgPool, order_id: &str) -> Result<()> {
    sqlx::query("UPDATE t_purchase_order SET status = 'NOTICE_CREATED', updated_at = NOW() WHERE order_id = $1")
        .bind(order_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 支払通知書を取得する
pub async fn find_payment_notice(pool: &PgPool, notice_id: &str) -> Result<Option<PaymentNotice>> {
    let row = sqlx::query_as::<_, PaymentNotice>(
        "SELECT * FROM t_payment_notice WHERE notice_id = $1"
    )
    .bind(notice_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// パートナー名を取得する
pub async fn find_partner_name(pool: &PgPool, partner_id: &str) -> Result<String> {
    let name: String = sqlx::query_scalar("SELECT name FROM m_partner WHERE partner_id = $1")
        .bind(partner_id)
        .fetch_one(pool)
        .await?;
    Ok(name)
}

/// 支払通知書明細一覧を取得する
pub async fn list_payment_notice_items(pool: &PgPool, notice_id: &str) -> Result<Vec<PaymentNoticeItem>> {
    let rows = sqlx::query_as::<_, PaymentNoticeItem>(
        "SELECT * FROM t_payment_notice_item WHERE notice_id = $1 ORDER BY id"
    )
    .bind(notice_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 自社情報（振込先銀行口座情報含む）を取得する
pub async fn find_company_bank_info(pool: &PgPool) -> Result<Option<(String, String, String, String, String, String)>> {
    let row = sqlx::query_as(
        "SELECT name, bank_name, bank_branch, account_type, account_number, account_name FROM s_company_info ORDER BY id LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 発注契約に紐づく技術者名を取得する（PDF明細の技術者名表示用）
pub async fn find_engineer_name_for_partner_contract(pool: &PgPool, partner_contract_id: i64) -> Result<Option<String>> {
    let name = sqlx::query_scalar(
        "SELECT e.name FROM m_partner_contract pc JOIN m_engineer e ON e.id = pc.engineer_id WHERE pc.id = $1"
    )
    .bind(partner_contract_id)
    .fetch_optional(pool)
    .await?;
    Ok(name)
}

/// 支払通知書一覧表示用の行
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct NoticeRow {
    pub notice_id: String,
    pub partner_name: String,
    pub target_month: chrono::NaiveDate,
    pub total: i32,
    pub confirmed: bool,
    pub notice_date: chrono::NaiveDate,
    pub purchase_order_id: String,
}

/// 支払通知書一覧（パートナー名JOIN済み、任意でpartner_idフィルタ、最大100件）
pub async fn list_notice_rows(pool: &PgPool, partner: Option<&str>) -> Result<Vec<NoticeRow>> {
    let mut sql = String::from(
        r#"
        SELECT pn.notice_id, p.name AS partner_name, pn.target_month,
               pn.total, (pn.partner_accepted_at IS NOT NULL) AS confirmed, pn.notice_date,
               pn.purchase_order_id
        FROM t_payment_notice pn
        JOIN m_partner p ON pn.partner_id = p.partner_id
        WHERE 1=1
        "#
    );
    if partner.is_some() {
        sql.push_str(" AND pn.partner_id = $1");
    }
    sql.push_str(" ORDER BY pn.created_at DESC LIMIT 100");

    let mut query = sqlx::query_as::<_, NoticeRow>(&sql);
    if let Some(p) = partner {
        query = query.bind(p);
    }
    Ok(query.fetch_all(pool).await?)
}

/// 受注書ヘッダー部分更新（JSON API用）の対象フィールド。全てOptionで、Someのものだけ更新する
#[derive(Debug, Default)]
pub struct ReceivedOrderUpdateFields<'a> {
    pub status: Option<&'a str>,
    pub target_month: Option<&'a str>,
    pub remarks: Option<&'a str>,
    pub project_name: Option<&'a str>,
    pub work_start: Option<&'a str>,
    pub work_end: Option<&'a str>,
    pub client_order_number: Option<&'a str>,
    pub payment_condition: Option<&'a str>,
    pub report_to_email: Option<&'a str>,
    pub report_cc_emails: Option<&'a str>,
    pub invoice_to_email: Option<&'a str>,
    pub invoice_cc_emails: Option<&'a str>,
}

impl ReceivedOrderUpdateFields<'_> {
    pub fn is_empty(&self) -> bool {
        self.status.is_none() && self.target_month.is_none() && self.remarks.is_none()
            && self.project_name.is_none() && self.work_start.is_none() && self.work_end.is_none()
            && self.client_order_number.is_none() && self.payment_condition.is_none()
            && self.report_to_email.is_none() && self.report_cc_emails.is_none()
            && self.invoice_to_email.is_none() && self.invoice_cc_emails.is_none()
    }
}

/// 受注書ヘッダーを部分更新する（JSON API用。指定されたフィールドのみ更新）
pub async fn update_received_order_partial(
    pool: &PgPool,
    id: i64,
    fields: &ReceivedOrderUpdateFields<'_>,
) -> Result<()> {
    let mut updates = Vec::new();
    let mut binds_str = Vec::new();
    let mut bind_idx = 1;

    // target_month/work_start/work_endはDATE列のため、text型で渡すパラメータには
    // 明示的に::dateキャストが必要（他はTEXT/VARCHAR列なのでキャスト不要）
    let columns: [(&str, Option<&str>, bool); 12] = [
        ("status", fields.status, false),
        ("target_month", fields.target_month, true),
        ("remarks", fields.remarks, false),
        ("project_name", fields.project_name, false),
        ("work_start", fields.work_start, true),
        ("work_end", fields.work_end, true),
        ("client_order_number", fields.client_order_number, false),
        ("payment_condition", fields.payment_condition, false),
        ("report_to_email", fields.report_to_email, false),
        ("report_cc_emails", fields.report_cc_emails, false),
        ("invoice_to_email", fields.invoice_to_email, false),
        ("invoice_cc_emails", fields.invoice_cc_emails, false),
    ];
    for (col, val, is_date) in columns {
        if let Some(v) = val {
            let cast = if is_date { "::date" } else { "" };
            updates.push(format!("{col} = ${bind_idx}{cast}"));
            binds_str.push(v.to_string());
            bind_idx += 1;
        }
    }

    if updates.is_empty() {
        return Ok(());
    }

    let sql = format!("UPDATE t_received_order SET {}, updated_at = NOW() WHERE id = ${}", updates.join(", "), bind_idx);

    let mut query = sqlx::query(&sql);
    for val in &binds_str {
        query = query.bind(val);
    }
    query = query.bind(id);
    query.execute(pool).await?;
    Ok(())
}

/// 受注明細の精算条件フィールドを更新する（SPA編集モーダル用。amountはunit_price*man_monthで再計算した値を渡す）
/// 既存の`update_received_order_item_fields`（旧SSR編集フォーム用、actual_hours/adjustment更新）とは別関数
#[allow(clippy::too_many_arguments)]
pub async fn update_received_order_item_settlement_fields(
    pool: &PgPool,
    item_id: i64,
    order_id: i64,
    unit_price: i32,
    man_month: Decimal,
    settlement_type: &str,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    deduction_rate: i32,
    overtime_rate: i32,
    amount: i32,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE t_received_order_item SET
            unit_price = $1, man_month = $2, settlement_type = $3,
            lower_limit_hours = $4, upper_limit_hours = $5,
            deduction_rate = $6, overtime_rate = $7, amount = $8
           WHERE id = $9 AND order_id = $10"#
    )
    .bind(unit_price)
    .bind(man_month)
    .bind(settlement_type)
    .bind(lower_limit_hours)
    .bind(upper_limit_hours)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(amount)
    .bind(item_id)
    .bind(order_id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── ロールフォワード用 ──

/// 対象月の同名プロジェクトの受注が存在するか（重複チェック）
pub async fn received_order_exists_for_client_month_project(
    pool: &PgPool,
    client_id: i64,
    target_month: chrono::NaiveDate,
    project_name: &str,
) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_received_order WHERE client_id = $1 AND target_month = $2 AND project_name = $3)"
    )
    .bind(client_id)
    .bind(target_month)
    .bind(project_name)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// ロールフォワード用受注を作成して ID を返す
pub async fn insert_rollforward_received_order(
    tx: &mut sqlx::PgConnection,
    received_order_no: &str,
    client_id: i64,
    engineer_id: Option<i64>,
    client_contract_id: Option<i64>,
    client_order_number: Option<&str>,
    target_month: chrono::NaiveDate,
    work_end: chrono::NaiveDate,
    project_name: &str,
    is_recurring: bool,
    parent_order_id: Option<i64>,
    remarks: &str,
    report_to_email: Option<&str>,
    report_cc_emails: Option<&str>,
    invoice_to_email: Option<&str>,
    invoice_cc_emails: Option<&str>,
) -> Result<i64> {
    let new_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            received_order_no, client_id, engineer_id, client_contract_id,
            client_order_number, target_month,
            work_start, work_end, project_name, status,
            is_recurring, parent_order_id, order_date, remarks,
            report_to_email, report_cc_emails, invoice_to_email, invoice_cc_emails
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'REGISTERED', $10, $11, CURRENT_DATE, $12, $13, $14, $15, $16)
        RETURNING id
        "#
    )
    .bind(received_order_no)
    .bind(client_id)
    .bind(engineer_id)
    .bind(client_contract_id)
    .bind(client_order_number)
    .bind(target_month)
    .bind(target_month)
    .bind(work_end)
    .bind(project_name)
    .bind(is_recurring)
    .bind(parent_order_id)
    .bind(remarks)
    .bind(report_to_email)
    .bind(report_cc_emails)
    .bind(invoice_to_email)
    .bind(invoice_cc_emails)
    .fetch_one(&mut *tx)
    .await?;
    Ok(new_id)
}

/// ロールフォワード用受注明細を作成
pub async fn insert_rollforward_received_order_item(
    tx: &mut sqlx::PgConnection,
    order_id: i64,
    client_contract_id: Option<i64>,
    engineer_name: &str,
    unit_price: i32,
    man_month: Decimal,
    settlement_type: &str,
    base_rate: i32,
    lower_limit_hours: Decimal,
    upper_limit_hours: Decimal,
    fixed_hours: Option<Decimal>,
    deduction_rate: i32,
    overtime_rate: i32,
    effort: Decimal,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_received_order_item (
            order_id, client_contract_id, engineer_name, unit_price, man_month,
            settlement_type, base_rate, lower_limit_hours, upper_limit_hours,
            fixed_hours, deduction_rate, overtime_rate, effort, amount
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                  ($4::NUMERIC * $5)::INTEGER)
        "#
    )
    .bind(order_id)
    .bind(client_contract_id)
    .bind(engineer_name)
    .bind(unit_price)
    .bind(man_month)
    .bind(settlement_type)
    .bind(base_rate)
    .bind(lower_limit_hours)
    .bind(upper_limit_hours)
    .bind(fixed_hours)
    .bind(deduction_rate)
    .bind(overtime_rate)
    .bind(effort)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// 継続受注一覧（ロールフォワード用）
pub async fn list_recurring_received_orders(pool: &PgPool) -> Result<Vec<ReceivedOrder>> {
    let rows = sqlx::query_as::<_, ReceivedOrder>(
        r#"
        SELECT DISTINCT ON (client_id, project_name) *
        FROM t_received_order
        WHERE is_recurring = true AND status IN ('REGISTERED', 'ACTIVE')
        ORDER BY client_id, project_name, target_month DESC
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 受注明細を取得（トランザクション版）
pub async fn list_received_order_items_tx(
    tx: &mut sqlx::PgConnection,
    order_id: i64,
) -> Result<Vec<ReceivedOrderItem>> {
    let rows = sqlx::query_as::<_, ReceivedOrderItem>(
        "SELECT * FROM t_received_order_item WHERE order_id = $1"
    )
    .bind(order_id)
    .fetch_all(&mut *tx)
    .await?;
    Ok(rows)
}

/// メールテスト用：最新の購買注文 UUID を取得
pub async fn find_latest_purchase_order_uuid(pool: &PgPool) -> Result<Option<String>> {
    let uuid: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT uuid::text FROM t_purchase_order ORDER BY created_at DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(uuid)
}

/// メールテスト用：最新の購買注文 order_id を取得
pub async fn find_latest_purchase_order_id(pool: &PgPool) -> Result<Option<String>> {
    let order_id: Option<String> = sqlx::query_scalar::<_, String>(
        "SELECT order_id FROM t_purchase_order ORDER BY created_at DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(order_id)
}

#[cfg(test)]
mod report_received_advance_tests {
    use super::is_pre_report_received_po_status;

    #[test]
    fn pre_report_received_allows_earlier_pipeline_statuses() {
        assert!(is_pre_report_received_po_status("DRAFT"));
        assert!(is_pre_report_received_po_status("SENT"));
        assert!(is_pre_report_received_po_status("ACCEPTED"));
    }

    #[test]
    fn pre_report_received_rejects_same_or_later_statuses() {
        assert!(!is_pre_report_received_po_status("REPORT_RECEIVED"));
        assert!(!is_pre_report_received_po_status("NOTICE_CREATED"));
        assert!(!is_pre_report_received_po_status("NOTICE_CONFIRMED"));
        assert!(!is_pre_report_received_po_status("PAID"));
        assert!(!is_pre_report_received_po_status("CANCELLED"));
        assert!(!is_pre_report_received_po_status(""));
    }
}
