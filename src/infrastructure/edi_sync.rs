/// infrastructure/edi_sync.rs — EDI→Sophia データ同期
///
/// Django版 core/management/commands/sync_from_edi.py (691行) の移植。
/// EDI DBからSophiaへ12テーブルを依存関係順に同期する。
///
/// ## 使い方
/// ```ignore
/// let sync = EdiSync::new(sophia_pool, edi_pool);
/// sync.run(false).await?;  // 差分同期
/// sync.run(true).await?;   // 全件再同期
/// ```

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::infrastructure::sync_lock::with_sync_lock;

/// 同期状態ファイルのパス
const SYNC_STATE_FILE: &str = ".edi_sync_state.json";

/// 同期状態
#[derive(Debug, Serialize, Deserialize)]
struct SyncState {
    last_sync: String,
}

/// EDI同期サービス
pub struct EdiSync {
    sophia: PgPool,
    edi: PgPool,
    last_sync: Option<DateTime<Utc>>,
    counters: HashMap<String, (i64, i64)>,
    dry_run: bool,
}

impl EdiSync {
    pub fn new(sophia: PgPool, edi: PgPool) -> Self {
        Self {
            sophia,
            edi,
            last_sync: None,
            counters: HashMap::new(),
            dry_run: false,
        }
    }

    /// 同期実行
    pub async fn run(&mut self, full: bool, dry_run: bool) -> Result<()> {
        self.dry_run = dry_run;
        self.last_sync = if full { None } else { self.load_last_sync() };

        if full {
            info!("=== 全件同期（初期移行）モード ===");
        } else {
            info!("=== 差分同期モード（前回: {:?}）===", self.last_sync);
        }

        let sync_start = Utc::now();

        // 排他制御下で同期実行
        let sophia = self.sophia.clone();
        let result = with_sync_lock(&sophia, self.run_sync()).await;

        // サマリー
        info!("=== 同期結果 ===");
        let mut total = 0i64;
        for (name, (created, updated)) in &self.counters {
            if *created > 0 || *updated > 0 {
                info!("  {}: 新規{}件 / 更新{}件", name, created, updated);
                total += created + updated;
            }
        }
        if total == 0 {
            info!("  変更なし");
        }

        if !self.dry_run {
            self.save_last_sync(sync_start);
            info!("同期完了: {}", sync_start);
        } else {
            warn!("[dry-run] 変更は適用されていません");
        }

        result
    }

    /// 全テーブルを依存関係順に同期
    async fn run_sync(&mut self) -> Result<()> {
        self.sync_company_info().await?;
        self.sync_clients().await?;
        self.sync_partners().await?;
        self.sync_projects().await?;
        self.sync_workplaces().await?;
        self.sync_partner_contracts().await?;
        self.sync_purchase_orders().await?;
        self.sync_purchase_order_items().await?;
        self.sync_received_orders().await?;
        self.sync_received_order_items().await?;
        self.sync_email_templates().await?;
        Ok(())
    }

    // ── 同期状態管理 ──

    fn load_last_sync(&self) -> Option<DateTime<Utc>> {
        let path = std::path::Path::new(SYNC_STATE_FILE);
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<SyncState>(&content) {
                return DateTime::parse_from_rfc3339(&state.last_sync)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc));
            }
        }
        None
    }

    fn save_last_sync(&self, dt: DateTime<Utc>) {
        let state = SyncState {
            last_sync: dt.to_rfc3339(),
        };
        if let Ok(json) = serde_json::to_string(&state) {
            let _ = std::fs::write(SYNC_STATE_FILE, json);
        }
    }

    fn where_updated(&self) -> (&str, Option<DateTime<Utc>>) {
        match self.last_sync {
            Some(dt) => ("WHERE updated_at > $1", Some(dt)),
            None => ("", None),
        }
    }

    // ── CompanyInfo ──

    async fn sync_company_info(&mut self) -> Result<()> {
        let rows: Vec<(String, String, String, String)> = sqlx::query_as(
            "SELECT COALESCE(name, ''), COALESCE(address, ''), COALESCE(tel, ''), COALESCE(representative_name, '') FROM core_companyinfo LIMIT 1"
        )
        .fetch_all(&self.edi)
        .await
        .unwrap_or_else(|e| { tracing::warn!("edi_sync: fetch_all failed: {:?}", e); vec![] });

        let mut created = 0i64;
        for (name, address, tel, rep) in &rows {
            if !self.dry_run {
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO s_company_info (name, address, tel, representative_name)
                       VALUES ($1, $2, $3, $4)
                       ON CONFLICT (id) DO UPDATE SET name = $1, address = $2, tel = $3, representative_name = $4"#
                )
                .bind(name).bind(address).bind(tel).bind(rep)
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
            }
            created += 1;
        }
        self.counters.insert("CompanyInfo".to_string(), (created, 0));
        info!("  CompanyInfo: +{}", created);
        Ok(())
    }

    // ── Client ──

    async fn sync_clients(&mut self) -> Result<()> {
        let (where_clause, _param) = self.where_updated();
        let sql = format!("SELECT * FROM core_client {}", where_clause);

        #[derive(sqlx::FromRow)]
        struct EdiClient {
            id: i64,
            name: String,
            postal_code: Option<String>,
            address: Option<String>,
            tel: Option<String>,
            email: Option<String>,
        }

        let rows = if let Some(dt) = self.last_sync {
            sqlx::query_as::<_, EdiClient>(&sql)
                .bind(dt)
                .fetch_all(&self.edi)
                .await?
        } else {
            sqlx::query_as::<_, EdiClient>(
                "SELECT id, name, COALESCE(postal_code, '') as postal_code, COALESCE(address, '') as address, COALESCE(tel, '') as tel, COALESCE(email, '') as email FROM core_client"
            )
            .fetch_all(&self.edi)
            .await?
        };

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM m_client WHERE client_id = $1::TEXT)")
                .bind(format!("C{:04}", r.id))
                .fetch_one(&self.sophia)
                .await
                .unwrap_or(false);

            if exists {
                if let Err(e) = sqlx::query(
                    "UPDATE m_client SET name = $1, postal_code = $2, address = $3, phone = $4, email = $5, updated_at = NOW() WHERE client_id = $6"
                )
                .bind(&r.name)
                .bind(r.postal_code.as_deref().unwrap_or(""))
                .bind(r.address.as_deref().unwrap_or(""))
                .bind(r.tel.as_deref().unwrap_or(""))
                .bind(r.email.as_deref().unwrap_or(""))
                .bind(format!("C{:04}", r.id))
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                updated += 1;
            } else {
                if let Err(e) = sqlx::query(
                    "INSERT INTO m_client (client_id, name, postal_code, address, phone, email) VALUES ($1, $2, $3, $4, $5, $6)"
                )
                .bind(format!("C{:04}", r.id))
                .bind(&r.name)
                .bind(r.postal_code.as_deref().unwrap_or(""))
                .bind(r.address.as_deref().unwrap_or(""))
                .bind(r.tel.as_deref().unwrap_or(""))
                .bind(r.email.as_deref().unwrap_or(""))
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("Client".to_string(), (created, updated));
        info!("  Client: +{} ~{}", created, updated);
        Ok(())
    }

    // ── Partner ──

    async fn sync_partners(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiPartner {
            partner_id: String,
            name: String,
            email: Option<String>,
            tel: Option<String>,
        }

        let rows = sqlx::query_as::<_, EdiPartner>(
            "SELECT partner_id, name, email, tel FROM core_customer"
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM m_partner WHERE partner_id = $1)")
                .bind(&r.partner_id)
                .fetch_one(&self.sophia)
                .await
                .unwrap_or(false);

            if exists {
                if let Err(e) = sqlx::query(
                    "UPDATE m_partner SET name = $1, email = $2, tel = $3, updated_at = NOW() WHERE partner_id = $4"
                )
                .bind(&r.name)
                .bind(r.email.as_deref().unwrap_or(""))
                .bind(r.tel.as_deref().unwrap_or(""))
                .bind(&r.partner_id)
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                updated += 1;
            } else {
                if let Err(e) = sqlx::query(
                    "INSERT INTO m_partner (partner_id, name, email, tel) VALUES ($1, $2, $3, $4)"
                )
                .bind(&r.partner_id)
                .bind(&r.name)
                .bind(r.email.as_deref().unwrap_or(""))
                .bind(r.tel.as_deref().unwrap_or(""))
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("Partner".to_string(), (created, updated));
        info!("  Partner: +{} ~{}", created, updated);
        Ok(())
    }

    // ── Project ──

    async fn sync_projects(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiProject {
            project_id: String,
            name: String,
        }

        let rows = sqlx::query_as::<_, EdiProject>(
            "SELECT project_id, name FROM orders_project"
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM m_project WHERE project_id = $1)")
                .bind(&r.project_id)
                .fetch_one(&self.sophia)
                .await
                .unwrap_or(false);

            if exists {
                if let Err(e) = sqlx::query("UPDATE m_project SET name = $1 WHERE project_id = $2")
                    .bind(&r.name).bind(&r.project_id)
                    .execute(&self.sophia).await { tracing::error!("DB error: {:?}", e); }
                updated += 1;
            } else {
                if let Err(e) = sqlx::query("INSERT INTO m_project (project_id, name) VALUES ($1, $2)")
                    .bind(&r.project_id).bind(&r.name)
                    .execute(&self.sophia).await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("Project".to_string(), (created, updated));
        info!("  Project: +{} ~{}", created, updated);
        Ok(())
    }

    // ── Workplace ──

    async fn sync_workplaces(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiWorkplace { id: i64, name: String }

        let rows = sqlx::query_as::<_, EdiWorkplace>(
            "SELECT id, name FROM orders_workplace"
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, _updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            if let Err(e) = sqlx::query(
                "INSERT INTO m_workplace (id, name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET name = $2"
            )
            .bind(r.id).bind(&r.name)
            .execute(&self.sophia)
            .await { tracing::error!("DB error: {:?}", e); }
            created += 1;
        }
        self.counters.insert("Workplace".to_string(), (created, 0));
        info!("  Workplace: +{}", created);
        Ok(())
    }

    // ── PartnerContract ──

    async fn sync_partner_contracts(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiContract {
            partner_id: String,
            project_id: String,
            person_name: String,
            base_fee: i32,
            settlement_type: Option<String>,
            time_lower_limit: Option<rust_decimal::Decimal>,
            time_upper_limit: Option<rust_decimal::Decimal>,
            shortage_rate: Option<i32>,
            excess_rate: Option<i32>,
            effort: Option<rust_decimal::Decimal>,
            work_start: NaiveDate,
            work_end: NaiveDate,
        }

        let rows = sqlx::query_as::<_, EdiContract>(
            r#"SELECT obi.partner_id, obi.project_id,
                      obii.person_name, obii.base_fee,
                      obii.settlement_type, obii.time_lower_limit,
                      obii.time_upper_limit, obii.shortage_rate, obii.excess_rate,
                      obii.effort,
                      obi.project_start_date AS work_start,
                      obi.project_end_date AS work_end
               FROM orders_orderbasicinfo obi
               JOIN orders_orderbasicinfoitem obii ON obii.basic_info_id = obi.id"#
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }

            // エンジニアを get_or_create（正規化名でマッチング — 表記ゆれ防止）
            let name_normalized = r.person_name
                .replace(' ', "").replace('　', "").replace('・', "");
            let eng_id: i64 = match sqlx::query_scalar::<_, i64>(
                "SELECT id FROM m_engineer WHERE name_normalized = $1"
            )
            .bind(&name_normalized)
            .fetch_optional(&self.sophia)
            .await?
            {
                Some(id) => id,
                None => {
                    sqlx::query_scalar::<_, i64>(
                        "INSERT INTO m_engineer (name, partner_id) VALUES ($1, $2) RETURNING id"
                    )
                    .bind(&r.person_name)
                    .bind(&r.partner_id)
                    .fetch_one(&self.sophia)
                    .await?
                }
            };

            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM m_partner_contract WHERE partner_id = $1 AND project_id = $2 AND engineer_id = $3)"
            )
            .bind(&r.partner_id).bind(&r.project_id).bind(eng_id)
            .fetch_one(&self.sophia)
            .await
            .unwrap_or(false);

            if exists {
                updated += 1;
            } else {
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO m_partner_contract (
                        partner_id, project_id, engineer_id, start_date, end_date,
                        base_rate, settlement_type, lower_limit_hours, upper_limit_hours,
                        deduction_rate, overtime_rate, effort, is_active
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, true)"#
                )
                .bind(&r.partner_id).bind(&r.project_id).bind(eng_id)
                .bind(r.work_start).bind(r.work_end)
                .bind(r.base_fee)
                .bind(r.settlement_type.as_deref().unwrap_or("RANGE"))
                .bind(r.time_lower_limit.unwrap_or(rust_decimal::Decimal::from(140)))
                .bind(r.time_upper_limit.unwrap_or(rust_decimal::Decimal::from(180)))
                .bind(r.shortage_rate.unwrap_or(0))
                .bind(r.excess_rate.unwrap_or(0))
                .bind(r.effort.unwrap_or(rust_decimal::Decimal::ONE))
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("PartnerContract".to_string(), (created, updated));
        info!("  PartnerContract: +{} ~{}", created, updated);
        Ok(())
    }

    // ── PurchaseOrder ──

    async fn sync_purchase_orders(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiOrder {
            order_id: String,
            uuid: uuid::Uuid,
            customer_id: String,
            project_id: String,
            order_date: NaiveDate,
            work_start: NaiveDate,
            work_end: NaiveDate,
            remarks: Option<String>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, EdiOrder>(
            "SELECT order_id, uuid, customer_id, project_id, order_date, work_start, work_end, remarks, created_at, updated_at FROM orders_order ORDER BY order_date"
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM t_purchase_order WHERE order_id = $1)"
            )
            .bind(&r.order_id)
            .fetch_one(&self.sophia)
            .await
            .unwrap_or(false);

            if exists {
                updated += 1;
            } else {
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO t_purchase_order (
                        order_id, uuid, partner_id, project_id, order_date,
                        work_start, work_end, remarks, status, created_at, updated_at
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'DRAFT', $9, $10)"#
                )
                .bind(&r.order_id).bind(r.uuid)
                .bind(&r.customer_id).bind(&r.project_id)
                .bind(r.order_date).bind(r.work_start).bind(r.work_end)
                .bind(r.remarks.as_deref().unwrap_or(""))
                .bind(r.created_at).bind(r.updated_at)
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("PurchaseOrder".to_string(), (created, updated));
        info!("  PurchaseOrder: +{} ~{}", created, updated);
        Ok(())
    }

    // ── PurchaseOrderItem ──

    async fn sync_purchase_order_items(&mut self) -> Result<()> {
        info!("  PurchaseOrderItem: スキップ（差分同期は未対応）");
        self.counters.insert("PurchaseOrderItem".to_string(), (0, 0));
        Ok(())
    }

    // ── ReceivedOrder ──

    async fn sync_received_orders(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiReceivedOrder {
            received_order_no: String,
            uuid: uuid::Uuid,
            target_month: NaiveDate,
            work_start: NaiveDate,
            work_end: NaiveDate,
            project_name: Option<String>,
            status: Option<String>,
            created_at: DateTime<Utc>,
            updated_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, EdiReceivedOrder>(
            "SELECT received_order_no, uuid, target_month, work_start, work_end, project_name, status, created_at, updated_at FROM billing_receivedorder ORDER BY target_month"
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM t_received_order WHERE received_order_no = $1)"
            )
            .bind(&r.received_order_no)
            .fetch_one(&self.sophia)
            .await
            .unwrap_or(false);

            if exists {
                updated += 1;
            } else {
                if let Err(e) = sqlx::query(
                    r#"INSERT INTO t_received_order (
                        received_order_no, uuid, target_month, work_start, work_end,
                        project_name, status, created_at, updated_at
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#
                )
                .bind(&r.received_order_no).bind(r.uuid)
                .bind(r.target_month).bind(r.work_start).bind(r.work_end)
                .bind(r.project_name.as_deref().unwrap_or(""))
                .bind(r.status.as_deref().unwrap_or("REGISTERED"))
                .bind(r.created_at).bind(r.updated_at)
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("ReceivedOrder".to_string(), (created, updated));
        info!("  ReceivedOrder: +{} ~{}", created, updated);
        Ok(())
    }

    // ── ReceivedOrderItem ──

    async fn sync_received_order_items(&mut self) -> Result<()> {
        info!("  ReceivedOrderItem: スキップ（差分同期は未対応）");
        self.counters.insert("ReceivedOrderItem".to_string(), (0, 0));
        Ok(())
    }

    // ── EmailTemplate ──

    async fn sync_email_templates(&mut self) -> Result<()> {
        #[derive(sqlx::FromRow)]
        struct EdiTemplate {
            code: String,
            subject: String,
            body: String,
            description: Option<String>,
        }

        let rows = sqlx::query_as::<_, EdiTemplate>(
            "SELECT COALESCE(code, '') as code, COALESCE(subject, '') as subject, COALESCE(body, '') as body, description FROM core_emailtemplate"
        )
        .fetch_all(&self.edi)
        .await?;

        let (mut created, mut updated) = (0i64, 0i64);
        for r in &rows {
            if self.dry_run { created += 1; continue; }
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM s_email_template WHERE code = $1)"
            )
            .bind(&r.code)
            .fetch_one(&self.sophia)
            .await
            .unwrap_or(false);

            if exists {
                if let Err(e) = sqlx::query(
                    "UPDATE s_email_template SET subject = $1, body = $2, description = $3 WHERE code = $4"
                )
                .bind(&r.subject).bind(&r.body)
                .bind(r.description.as_deref().unwrap_or(""))
                .bind(&r.code)
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                updated += 1;
            } else {
                if let Err(e) = sqlx::query(
                    "INSERT INTO s_email_template (code, subject, body, description) VALUES ($1, $2, $3, $4)"
                )
                .bind(&r.code).bind(&r.subject).bind(&r.body)
                .bind(r.description.as_deref().unwrap_or(""))
                .execute(&self.sophia)
                .await { tracing::error!("DB error: {:?}", e); }
                created += 1;
            }
        }
        self.counters.insert("EmailTemplate".to_string(), (created, updated));
        info!("  EmailTemplate: +{} ~{}", created, updated);
        Ok(())
    }
}
