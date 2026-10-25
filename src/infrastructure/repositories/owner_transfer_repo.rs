/// infrastructure/repositories/owner_transfer_repo.rs — 担当(スタンプ)の付け替え(DEMO-000173 PR 6b。設計書 D15・U11)
///
/// - 顧客・案件を起点に、配下のデータの担当をまとめて付け替える(子は親に従う)。
/// - 社員を起点に、その社員が担当のデータをすべて付け替える(退職時の引き継ぎ)。
///
/// どちらも、呼び出し側が開いたトランザクション(管理操作のロック済み)の中で呼ぶ。更新は「親 → 子」の順。
/// 担当組織・担当者の2列は、いつも一緒に更新する(片方だけの穴を作らない)。
use anyhow::Result;
use serde::Serialize;
use sqlx::{PgConnection, PgExecutor};

pub use crate::domain::authz::owner_transfer::TransferRoot;
use crate::domain::authz::stamp::OwnerStamp;

/// テーブルごとの件数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TableCount {
    pub table: &'static str,
    pub count: i64,
}

/// 件数の合計。
pub fn total(counts: &[TableCount]) -> i64 {
    counts.iter().map(|c| c.count).sum()
}

/// 担当スタンプを持つテーブルのSQLを、テーブル名ごとに固定の文字列として作る(動的なSQL結合をしない)。
macro_rules! stamped_tables {
    ($($t:literal),+ $(,)?) => {
        &[$(($t,
            concat!("UPDATE ", $t, " SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE owner_employee_id = $3"),
            concat!("SELECT COUNT(*) FROM ", $t, " WHERE owner_employee_id = $1"),
        )),+]
    };
}

/// 担当スタンプを持つ全テーブル(テーブル名, 付け替えSQL, 件数SQL)。親 → 子の順。
/// 稼働報告の自己申告の行は担当が NULL なので、どちらにも当たらない。
const STAMPED: &[(&str, &str, &str)] = stamped_tables![
    "m_client",
    "m_project",
    "m_client_contract",
    "m_partner_contract",
    "t_purchase_order",
    "t_payment_notice",
    "t_monthly_task",
    "t_monthly_timesheet",
    "t_received_order",
    "t_billing_invoice",
];

// ── 起点の存在確認と、付け替え前のスタンプ ──

/// 起点の行の今のスタンプ。起点が無ければ `None`。
pub async fn root_stamp<'e>(exec: impl PgExecutor<'e>, root: &TransferRoot) -> Result<Option<OwnerStamp>> {
    let row: Option<(Option<i64>, Option<i64>)> = match root {
        TransferRoot::Client(id) => {
            sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM m_client WHERE id = $1").bind(id).fetch_optional(exec).await?
        }
        TransferRoot::Project(pid) => {
            sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM m_project WHERE project_id = $1").bind(pid).fetch_optional(exec).await?
        }
    };
    Ok(row.map(|(org_unit_id, employee_id)| OwnerStamp { org_unit_id, employee_id }))
}

/// 社員が有効かどうか。社員がいなければ `None`。
pub async fn employee_is_active<'e>(exec: impl PgExecutor<'e>, employee_id: i64) -> Result<Option<bool>> {
    Ok(sqlx::query_scalar("SELECT is_active FROM m_employee WHERE id = $1").bind(employee_id).fetch_optional(exec).await?)
}

// ── 顧客・案件を起点にした付け替え ──

async fn run(conn: &mut PgConnection, out: &mut Vec<TableCount>, table: &'static str, sql: &'static str, to: &OwnerStamp, key: Key<'_>) -> Result<()> {
    let q = sqlx::query(sql).bind(to.org_unit_id).bind(to.employee_id);
    let q = match key {
        Key::Client(id) => q.bind(id),
        Key::Projects(ids) => q.bind(ids),
    };
    let n = q.execute(&mut *conn).await?.rows_affected();
    out.push(TableCount { table, count: n as i64 });
    Ok(())
}

/// SQL の `$3` に渡す起点。
#[derive(Clone, Copy)]
enum Key<'a> {
    Client(i64),
    Projects(&'a [String]),
}

/// 案件の配下(契約・発注・支払通知・月次タスク・稼働報告)の付け替え。`$3` は案件IDの配列。
const PROJECT_CHILDREN: &[(&str, &str)] = &[
    ("m_client_contract", "UPDATE m_client_contract SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE project_id = ANY($3::text[])"),
    ("m_partner_contract", "UPDATE m_partner_contract SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE project_id = ANY($3::text[])"),
    ("t_purchase_order", "UPDATE t_purchase_order SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE project_id = ANY($3::text[])"),
    (
        "t_payment_notice",
        "UPDATE t_payment_notice SET owner_org_unit_id = $1, owner_employee_id = $2 \
         WHERE purchase_order_id IN (SELECT order_id FROM t_purchase_order WHERE project_id = ANY($3::text[]))",
    ),
    ("t_monthly_task", "UPDATE t_monthly_task SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE project_id = ANY($3::text[])"),
    (
        "t_monthly_timesheet",
        "UPDATE t_monthly_timesheet SET owner_org_unit_id = $1, owner_employee_id = $2 \
         WHERE employee_id IS NULL AND client_contract_id IN (SELECT id FROM m_client_contract WHERE project_id = ANY($3::text[]))",
    ),
];

/// 顧客の、受注と請求書(顧客IDで引く。契約や案件に結びつかないものも含む)。
const CLIENT_ORDERS: &[(&str, &str)] = &[
    ("t_received_order", "UPDATE t_received_order SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE client_id = $3"),
    ("t_billing_invoice", "UPDATE t_billing_invoice SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE client_id = $3"),
];

/// 案件の、受注と請求書(契約の案件・請求書の案件・請求書の受注の案件で引く)。
const PROJECT_ORDERS: &[(&str, &str)] = &[
    (
        "t_received_order",
        "UPDATE t_received_order SET owner_org_unit_id = $1, owner_employee_id = $2 \
         WHERE client_contract_id IN (SELECT id FROM m_client_contract WHERE project_id = ANY($3::text[]))",
    ),
    (
        "t_billing_invoice",
        "UPDATE t_billing_invoice SET owner_org_unit_id = $1, owner_employee_id = $2 \
         WHERE project_id = ANY($3::text[]) OR received_order_id IN (SELECT id FROM t_received_order \
            WHERE client_contract_id IN (SELECT id FROM m_client_contract WHERE project_id = ANY($3::text[])))",
    ),
];

/// 起点の行と配下のデータの担当を、`to` に付け替える。テーブルごとの更新件数を、親 → 子の順で返す。
/// 起点が存在するかは、呼び出し側が先に `root_stamp` で確かめる。
pub async fn transfer_subtree(conn: &mut PgConnection, root: &TransferRoot, to: &OwnerStamp) -> Result<Vec<TableCount>> {
    let mut out = Vec::new();
    match root {
        TransferRoot::Client(id) => {
            run(conn, &mut out, "m_client", "UPDATE m_client SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE id = $3", to, Key::Client(*id)).await?;
            run(conn, &mut out, "m_project", "UPDATE m_project SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE client_id = $3", to, Key::Client(*id))
                .await?;
            let ids: Vec<String> = sqlx::query_scalar("SELECT project_id FROM m_project WHERE client_id = $1 ORDER BY project_id").bind(id).fetch_all(&mut *conn).await?;
            for (table, sql) in PROJECT_CHILDREN {
                run(conn, &mut out, table, sql, to, Key::Projects(&ids)).await?;
            }
            for (table, sql) in CLIENT_ORDERS {
                run(conn, &mut out, table, sql, to, Key::Client(*id)).await?;
            }
        }
        TransferRoot::Project(pid) => {
            let ids = vec![pid.clone()];
            run(conn, &mut out, "m_project", "UPDATE m_project SET owner_org_unit_id = $1, owner_employee_id = $2 WHERE project_id = ANY($3::text[])", to, Key::Projects(&ids))
                .await?;
            for (table, sql) in PROJECT_CHILDREN.iter().chain(PROJECT_ORDERS.iter()) {
                run(conn, &mut out, table, sql, to, Key::Projects(&ids)).await?;
            }
        }
    }
    Ok(out)
}

// ── 社員を起点にした付け替え ──

/// 社員が担当のデータの件数(テーブルごと。0件のテーブルも含む)。
pub async fn count_owned_by_employee(conn: &mut PgConnection, employee_id: i64) -> Result<Vec<TableCount>> {
    let mut out = Vec::with_capacity(STAMPED.len());
    for (table, _, count_sql) in STAMPED {
        let n: i64 = sqlx::query_scalar(count_sql).bind(employee_id).fetch_one(&mut *conn).await?;
        out.push(TableCount { table, count: n });
    }
    // 自動取込の受け皿(会社設定。D16)も、担当として数える
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM s_company_info WHERE auto_import_owner_employee_id = $1").bind(employee_id).fetch_one(&mut *conn).await?;
    out.push(TableCount { table: "s_company_info", count: n });
    Ok(out)
}

/// 担当の表示用(組織名・担当者名つき)。
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct OwnerView {
    pub org_unit_id: Option<i64>,
    pub org_name: Option<String>,
    pub employee_id: Option<i64>,
    pub employee_name: Option<String>,
}

/// 顧客の今の担当。顧客が無ければ None。
pub async fn owner_view_of_client(pool: &sqlx::PgPool, client_id: i64) -> Result<Option<OwnerView>> {
    Ok(sqlx::query_as::<_, OwnerView>(
        "SELECT c.owner_org_unit_id AS org_unit_id, o.name AS org_name, c.owner_employee_id AS employee_id, \
                CASE WHEN e.id IS NULL THEN NULL ELSE e.last_name || ' ' || e.first_name END AS employee_name \
         FROM m_client c LEFT JOIN m_org_unit o ON o.id = c.owner_org_unit_id LEFT JOIN m_employee e ON e.id = c.owner_employee_id \
         WHERE c.id = $1",
    )
    .bind(client_id)
    .fetch_optional(pool)
    .await?)
}

/// 案件の今の担当。案件が無ければ None。
pub async fn owner_view_of_project(pool: &sqlx::PgPool, project_id: &str) -> Result<Option<OwnerView>> {
    Ok(sqlx::query_as::<_, OwnerView>(
        "SELECT p.owner_org_unit_id AS org_unit_id, o.name AS org_name, p.owner_employee_id AS employee_id, \
                CASE WHEN e.id IS NULL THEN NULL ELSE e.last_name || ' ' || e.first_name END AS employee_name \
         FROM m_project p LEFT JOIN m_org_unit o ON o.id = p.owner_org_unit_id LEFT JOIN m_employee e ON e.id = p.owner_employee_id \
         WHERE p.project_id = $1",
    )
    .bind(project_id)
    .fetch_optional(pool)
    .await?)
}

/// 自動取込の受け皿(会社設定。D16)を設定する。会社情報は常に1行。
pub async fn set_auto_import_receiver(conn: &mut PgConnection, to: &OwnerStamp) -> Result<()> {
    sqlx::query("UPDATE s_company_info SET auto_import_owner_org_unit_id = $1, auto_import_owner_employee_id = $2")
        .bind(to.org_unit_id)
        .bind(to.employee_id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

/// 組織ごとの件数(引き継ぎの確認画面で、どの組織のデータが動くかを見せるため)。
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct OrgCount {
    pub org_unit_id: Option<i64>,
    pub org_name: Option<String>,
    pub count: i64,
}

/// 社員が担当のデータを、担当組織ごとに数える(全スタンプ表の合計)。
pub async fn count_owned_by_org_for_employee(conn: &mut PgConnection, employee_id: i64) -> Result<Vec<OrgCount>> {
    let rows = sqlx::query_as::<_, OrgCount>(
        "SELECT s.org_unit_id, o.name AS org_name, COUNT(*)::BIGINT AS count FROM ( \
           SELECT owner_org_unit_id AS org_unit_id FROM m_client WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM m_project WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM m_client_contract WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM m_partner_contract WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM t_purchase_order WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM t_payment_notice WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM t_received_order WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM t_billing_invoice WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM t_monthly_timesheet WHERE owner_employee_id = $1 \
           UNION ALL SELECT owner_org_unit_id FROM t_monthly_task WHERE owner_employee_id = $1 \
         ) s LEFT JOIN m_org_unit o ON o.id = s.org_unit_id \
         GROUP BY s.org_unit_id, o.name ORDER BY count DESC, s.org_unit_id",
    )
    .bind(employee_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

/// 社員が担当のデータすべての担当を、`to` に付け替える(退職時の引き継ぎ。D15)。
pub async fn transfer_by_employee(conn: &mut PgConnection, from_employee_id: i64, to: &OwnerStamp) -> Result<Vec<TableCount>> {
    let mut out = Vec::with_capacity(STAMPED.len());
    for (table, sql, _) in STAMPED {
        let n = sqlx::query(sql).bind(to.org_unit_id).bind(to.employee_id).bind(from_employee_id).execute(&mut *conn).await?.rows_affected();
        out.push(TableCount { table, count: n as i64 });
    }
    // 自動取込の受け皿(D16)が退職者なら、引き継ぎ先へ移す
    let n = sqlx::query(
        "UPDATE s_company_info SET auto_import_owner_org_unit_id = $1, auto_import_owner_employee_id = $2 WHERE auto_import_owner_employee_id = $3",
    )
    .bind(to.org_unit_id)
    .bind(to.employee_id)
    .bind(from_employee_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    out.push(TableCount { table: "s_company_info", count: n as i64 });
    Ok(out)
}
