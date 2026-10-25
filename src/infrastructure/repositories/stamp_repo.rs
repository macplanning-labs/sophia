/// infrastructure/repositories/stamp_repo.rs — データの担当スタンプを、親から読んで決める(設計書 §6.5・§6.6)
///
/// 新しいデータを作る経路は、この関数で `OwnerStamp` を得て、INSERT の
/// `owner_org_unit_id` / `owner_employee_id` に入れる。決め方は純粋関数
/// `domain::authz::stamp::stamp_for`(近い親から順に、組織と担当者が揃った最初のもの)。
/// 親は作成と同じトランザクションで読む(呼び出し側が同じ接続を渡す)。
///
/// 決められないとき(設計書 U10。PR 7a):
/// - 手動作成(`StampMode::Manual`): 拒否する。`NoOwnerError` を `sqlx::Error::Decode` に包んで返す
///   (ハンドラは `domain::authz::stamp::is_no_owner` で見つけて 400 にする)
/// - 自動取込(`StampMode::AutoImport`): 受け皿(D16)が最後の親。それも未設定なら、エラーログを残して拒否する
///   (PR 7b で担当は必須になった。担当なしで作ろうとしても DB が拒否するので、分かる形で先に止める)

use sqlx::{PgConnection, PgExecutor};

use crate::domain::authz::stamp::{stamp_for, NoOwnerError, OwnerStamp, StampError, StampMode};

type StampRow = (Option<i64>, Option<i64>);

fn to_stamp(row: Option<StampRow>) -> OwnerStamp {
    row.map(|(org_unit_id, employee_id)| OwnerStamp { org_unit_id, employee_id }).unwrap_or_default()
}

/// 決められなかった(手動作成で、拒否が有効なときだけ起こる)場合を、型付きエラーとして DBエラーに載せる。
/// 検出は `domain::authz::stamp::is_no_owner`(原因の連鎖を辿る)。
fn no_owner(e: StampError) -> sqlx::Error {
    match e {
        StampError::NoOwner => sqlx::Error::Decode(Box::new(NoOwnerError)),
    }
}

/// スタンプを決める。自動取込で受け皿も無く担当が決まらないときは、エラーログを残して拒否する(個人情報は出さない)。
/// 担当は必須(マイグレーション 073)なので、担当なしでは作れない。
fn decide(mode: StampMode, table: &'static str, parents: &[OwnerStamp]) -> Result<OwnerStamp, sqlx::Error> {
    let stamp = stamp_for(mode, None, parents).map_err(no_owner)?;
    if !stamp.is_complete() {
        tracing::error!("[担当スタンプ] 担当が決まらないため作成しませんでした | table={table} 対応=設定 > 組織 の「自動取込の受け皿」を設定してください");
        return Err(no_owner(StampError::NoOwner));
    }
    Ok(stamp)
}

// ── 1行分のスタンプを読む(親が無い・NULL のときは担当なし) ──

/// 顧客のスタンプ。
pub async fn of_client<'e>(exec: impl PgExecutor<'e>, client_id: i64) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM m_client WHERE id = $1")
        .bind(client_id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 案件のスタンプ。
pub async fn of_project<'e>(exec: impl PgExecutor<'e>, project_id: &str) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM m_project WHERE project_id = $1")
        .bind(project_id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 受注契約のスタンプ。
pub async fn of_client_contract<'e>(exec: impl PgExecutor<'e>, id: i64) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM m_client_contract WHERE id = $1")
        .bind(id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 受注契約のスタンプを、まとめて読む(精算ダッシュボードの行の絞り込み・一括発行の判定用)。無い ID は結果に入らない。
pub async fn of_client_contracts<'e>(exec: impl PgExecutor<'e>, ids: &[i64]) -> Result<std::collections::HashMap<i64, OwnerStamp>, sqlx::Error> {
    let rows: Vec<(i64, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT id, owner_org_unit_id, owner_employee_id FROM m_client_contract WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(exec)
            .await?;
    Ok(rows.into_iter().map(|(id, o, e)| (id, OwnerStamp { org_unit_id: o, employee_id: e })).collect())
}

/// 発注契約のスタンプを、まとめて読む。無い ID は結果に入らない。
pub async fn of_partner_contracts<'e>(exec: impl PgExecutor<'e>, ids: &[i64]) -> Result<std::collections::HashMap<i64, OwnerStamp>, sqlx::Error> {
    let rows: Vec<(i64, Option<i64>, Option<i64>)> =
        sqlx::query_as("SELECT id, owner_org_unit_id, owner_employee_id FROM m_partner_contract WHERE id = ANY($1)")
            .bind(ids)
            .fetch_all(exec)
            .await?;
    Ok(rows.into_iter().map(|(id, o, e)| (id, OwnerStamp { org_unit_id: o, employee_id: e })).collect())
}

/// 発注契約のスタンプ。
pub async fn of_partner_contract<'e>(exec: impl PgExecutor<'e>, id: i64) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM m_partner_contract WHERE id = $1")
        .bind(id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 発注書のスタンプ(業務ID `order_id` で引く。支払通知・明細が持つ参照と同じ)。
pub async fn of_purchase_order<'e>(exec: impl PgExecutor<'e>, order_id: &str) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_purchase_order WHERE order_id = $1")
        .bind(order_id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 受注のスタンプ。
pub async fn of_received_order<'e>(exec: impl PgExecutor<'e>, id: i64) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_received_order WHERE id = $1")
        .bind(id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 受注書のスタンプ(数値なら ID、そうでなければ受注番号で引く。`update_received_order_status_by_id_or_no` と同じ)。
pub async fn of_received_order_by_id_or_no<'e>(exec: impl PgExecutor<'e>, id_or_no: &str) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = if let Ok(id) = id_or_no.parse::<i64>() {
        sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_received_order WHERE id = $1").bind(id).fetch_optional(exec).await?
    } else {
        sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_received_order WHERE received_order_no = $1").bind(id_or_no).fetch_optional(exec).await?
    };
    Ok(to_stamp(row))
}

/// 請求書のスタンプ。
pub async fn of_invoice<'e>(exec: impl PgExecutor<'e>, id: i64) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_billing_invoice WHERE id = $1")
        .bind(id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 支払通知のスタンプ(業務ID `notice_id` で引く)。
pub async fn of_payment_notice<'e>(exec: impl PgExecutor<'e>, notice_id: &str) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_payment_notice WHERE notice_id = $1")
        .bind(notice_id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 月次勤務表のスタンプ。
pub async fn of_timesheet<'e>(exec: impl PgExecutor<'e>, id: i64) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as("SELECT owner_org_unit_id, owner_employee_id FROM t_monthly_timesheet WHERE id = $1")
        .bind(id)
        .fetch_optional(exec)
        .await?;
    Ok(to_stamp(row))
}

/// 自動取込の受け皿(会社設定。D16)。未設定なら担当なし。
pub async fn auto_import_receiver<'e>(exec: impl PgExecutor<'e>) -> Result<OwnerStamp, sqlx::Error> {
    let row: Option<StampRow> = sqlx::query_as(
        "SELECT auto_import_owner_org_unit_id, auto_import_owner_employee_id FROM s_company_info ORDER BY id LIMIT 1",
    )
    .fetch_optional(exec)
    .await?;
    Ok(to_stamp(row))
}

// ── 作るデータごとの決め方(設計書 §6.6。近い親から順) ──

/// 案件 → その顧客、の順に並べる(案件が無ければ空)。
async fn project_chain(conn: &mut PgConnection, project_id: &str) -> Result<Vec<OwnerStamp>, sqlx::Error> {
    let row: Option<(Option<i64>, Option<i64>, Option<i64>, Option<i64>)> = sqlx::query_as(
        r#"
        SELECT p.owner_org_unit_id, p.owner_employee_id, c.owner_org_unit_id, c.owner_employee_id
          FROM m_project p
          LEFT JOIN m_client c ON c.id = p.client_id
         WHERE p.project_id = $1
        "#,
    )
    .bind(project_id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(match row {
        Some((po, pe, co, ce)) => vec![
            OwnerStamp { org_unit_id: po, employee_id: pe },
            OwnerStamp { org_unit_id: co, employee_id: ce },
        ],
        None => Vec::new(),
    })
}

/// 受注契約が属する案件のID。
async fn project_of_client_contract(conn: &mut PgConnection, contract_id: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT project_id FROM m_client_contract WHERE id = $1")
        .bind(contract_id)
        .fetch_optional(&mut *conn)
        .await
}

/// 受け皿を末尾に足す(自動取込のとき)。
async fn push_receiver(conn: &mut PgConnection, parents: &mut Vec<OwnerStamp>, mode: StampMode) -> Result<(), sqlx::Error> {
    if mode.is_auto_import() {
        parents.push(auto_import_receiver(&mut *conn).await?);
    }
    Ok(())
}

/// 顧客(EDI 同期で自動作成)。受け皿を写す(自動取込)。画面からの登録は、担当の指定が必須(`clients::create`)。
pub async fn for_auto_client(conn: &mut PgConnection) -> Result<OwnerStamp, sqlx::Error> {
    let receiver = auto_import_receiver(&mut *conn).await?;
    decide(StampMode::AutoImport, "m_client", &[receiver])
}

/// 案件: 顧客から写す(自動取込なら受け皿が最後)。
pub async fn for_project(conn: &mut PgConnection, client_id: Option<i64>, mode: StampMode) -> Result<OwnerStamp, sqlx::Error> {
    let mut parents = Vec::new();
    if let Some(cid) = client_id {
        parents.push(of_client(&mut *conn, cid).await?);
    }
    push_receiver(conn, &mut parents, mode).await?;
    decide(mode, "m_project", &parents)
}

/// 案件の子(契約・発注・月次タスク): 案件 → その顧客(自動取込なら受け皿が最後)。
pub async fn for_project_child(conn: &mut PgConnection, project_id: &str, table: &'static str, mode: StampMode) -> Result<OwnerStamp, sqlx::Error> {
    let mut parents = project_chain(conn, project_id).await?;
    push_receiver(conn, &mut parents, mode).await?;
    decide(mode, table, &parents)
}

/// 支払通知: 発注 → 発注の案件 → その顧客。
pub async fn for_payment_notice(conn: &mut PgConnection, purchase_order_id: &str, mode: StampMode) -> Result<OwnerStamp, sqlx::Error> {
    let mut parents = vec![of_purchase_order(&mut *conn, purchase_order_id).await?];
    let project_id: Option<String> = sqlx::query_scalar("SELECT project_id FROM t_purchase_order WHERE order_id = $1")
        .bind(purchase_order_id)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(pid) = project_id {
        parents.extend(project_chain(conn, &pid).await?);
    }
    push_receiver(conn, &mut parents, mode).await?;
    decide(mode, "t_payment_notice", &parents)
}

/// 受注: 結びついた契約の案件 → 顧客(自動取込なら受け皿が最後)。
pub async fn for_received_order(
    conn: &mut PgConnection,
    client_id: Option<i64>,
    client_contract_id: Option<i64>,
    mode: StampMode,
) -> Result<OwnerStamp, sqlx::Error> {
    let mut parents = Vec::new();
    if let Some(cc) = client_contract_id {
        if let Some(pid) = project_of_client_contract(conn, cc).await? {
            parents.push(of_project(&mut *conn, &pid).await?);
        }
    }
    if let Some(cid) = client_id {
        parents.push(of_client(&mut *conn, cid).await?);
    }
    push_receiver(conn, &mut parents, mode).await?;
    decide(mode, "t_received_order", &parents)
}

/// 請求書: 受注 → 案件 → 顧客。
pub async fn for_invoice(
    conn: &mut PgConnection,
    received_order_id: Option<i64>,
    project_id: Option<&str>,
    client_id: i64,
    mode: StampMode,
) -> Result<OwnerStamp, sqlx::Error> {
    let mut parents = Vec::new();
    if let Some(ro) = received_order_id {
        parents.push(of_received_order(&mut *conn, ro).await?);
    }
    if let Some(pid) = project_id {
        parents.push(of_project(&mut *conn, pid).await?);
    }
    parents.push(of_client(&mut *conn, client_id).await?);
    push_receiver(conn, &mut parents, mode).await?;
    decide(mode, "t_billing_invoice", &parents)
}

/// 稼働報告: 受注契約の案件 → 顧客(自動取込なら受け皿が最後)。
pub async fn for_timesheet(conn: &mut PgConnection, client_contract_id: Option<i64>, mode: StampMode) -> Result<OwnerStamp, sqlx::Error> {
    let mut parents = Vec::new();
    if let Some(cc) = client_contract_id {
        if let Some(pid) = project_of_client_contract(conn, cc).await? {
            parents = project_chain(conn, &pid).await?;
        }
    }
    push_receiver(conn, &mut parents, mode).await?;
    decide(mode, "t_monthly_timesheet", &parents)
}
