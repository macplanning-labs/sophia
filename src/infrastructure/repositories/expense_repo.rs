/// infrastructure/repositories/expense_repo.rs — 経費申請(t_expense_request + t_expense_request_item) CRUD
///
/// 1申請(ヘッダー)は複数の明細を持つ。ヘッダーの`total_amount`は明細の増減時に
/// 都度再計算する非正規化列（一覧表示で明細をJOIN集計せずに済ませるため）。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::expense::{ExpenseRequest, ExpenseRequestForm, ExpenseRequestItem, ExpenseRequestItemForm};

/// 一覧表示用の行（社員名JOIN済み・明細件数付き）
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ExpenseRow {
    pub id: i64,
    pub status: String,
    pub total_amount: i32,
    pub item_count: i64,
    pub employee_name: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 社員セレクト用
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct EmployeeOption {
    pub id: i64,
    pub display_name: String,
}

/// 経費科目選択肢
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ExpenseCategoryOption {
    pub code: String,
    pub name: String,
}

// ── ヘッダー ──

/// 経費申請ヘッダー+明細を作成する（SPA用JSON API。ステータスはPENDING固定）。
/// 作成されたヘッダーidと、各明細idを渡された順で返す。
pub async fn insert_pending(pool: &PgPool, employee_id: i64, items: &[ExpenseRequestItemForm]) -> Result<(i64, Vec<i64>)> {
    let mut tx = pool.begin().await?;

    let header_id: i64 = sqlx::query_scalar(
        "INSERT INTO t_expense_request (employee_id, status, total_amount, created_at) VALUES ($1, 'PENDING', 0, NOW()) RETURNING id"
    )
    .bind(employee_id)
    .fetch_one(&mut *tx)
    .await?;

    let mut item_ids = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let item_id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO t_expense_request_item
                (expense_request_id, expense_date, category, description, amount, display_order)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#
        )
        .bind(header_id)
        .bind(item.expense_date)
        .bind(&item.category)
        .bind(&item.description)
        .bind(item.amount)
        .bind(i as i32)
        .fetch_one(&mut *tx)
        .await?;
        item_ids.push(item_id);
    }

    recompute_total_tx(&mut tx, header_id).await?;
    tx.commit().await?;

    Ok((header_id, item_ids))
}

/// 経費申請ヘッダーを取得する
pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<ExpenseRequest>> {
    let row = sqlx::query_as::<_, ExpenseRequest>(
        "SELECT * FROM t_expense_request WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 経費申請ヘッダーの担当社員を変更する（PENDINGのもののみ。更新件数を返す）
pub async fn update_employee(pool: &PgPool, id: i64, employee_id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_expense_request SET employee_id = $2, updated_at = NOW() WHERE id = $1 AND status = 'PENDING'"
    )
    .bind(id)
    .bind(employee_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 経費申請ヘッダー削除（PENDINGのもののみ。明細はON DELETE CASCADEで自動削除。更新件数を返す）
pub async fn delete_pending(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM t_expense_request WHERE id = $1 AND status = 'PENDING'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 経費申請を承認する（PENDINGのもののみ。更新件数を返す）
pub async fn approve_pending(pool: &PgPool, id: i64, approved_by_id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_expense_request SET status = 'APPROVED', approved_by_id = $2, approved_at = NOW(), updated_at = NOW() WHERE id = $1 AND status = 'PENDING'"
    )
        .bind(id)
        .bind(approved_by_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// 経費申請を差戻す（PENDINGのもののみ。更新件数を返す）
pub async fn reject_pending(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("UPDATE t_expense_request SET status = 'REJECTED', updated_at = NOW() WHERE id = $1 AND status = 'PENDING'")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// ワークフロー遷移に伴うステータス更新（トランザクション内）。
/// `from_status` と一致する行のみ更新し、0件ならエラー。
pub async fn apply_workflow_status_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: i64,
    from_status: &str,
    to_status: &str,
    approved_by_id: Option<i64>,
) -> Result<()> {
    let result = match to_status {
        "APPROVED" => {
            sqlx::query(
                r#"
                UPDATE t_expense_request
                SET status = 'APPROVED',
                    approved_by_id = $3,
                    approved_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1 AND status = $2
                "#,
            )
            .bind(id)
            .bind(from_status)
            .bind(approved_by_id)
            .execute(&mut **tx)
            .await?
        }
        "PENDING" | "REJECTED" | "PAID" | "DRAFT" => {
            // 承認取り消し・差戻し・再申請時は承認メタデータをクリア
            let clear_approval = matches!(to_status, "PENDING" | "REJECTED" | "DRAFT");
            if clear_approval {
                sqlx::query(
                    r#"
                    UPDATE t_expense_request
                    SET status = $3,
                        approved_by_id = NULL,
                        approved_at = NULL,
                        updated_at = NOW()
                    WHERE id = $1 AND status = $2
                    "#,
                )
                .bind(id)
                .bind(from_status)
                .bind(to_status)
                .execute(&mut **tx)
                .await?
            } else {
                sqlx::query(
                    r#"
                    UPDATE t_expense_request
                    SET status = $3, updated_at = NOW()
                    WHERE id = $1 AND status = $2
                    "#,
                )
                .bind(id)
                .bind(from_status)
                .bind(to_status)
                .execute(&mut **tx)
                .await?
            }
        }
        other => anyhow::bail!("未対応の経費ステータスです: {other}"),
    };

    if result.rows_affected() == 0 {
        anyhow::bail!("ステータスを更新できませんでした（他のユーザーにより変更された可能性があります）");
    }
    Ok(())
}

/// 明細の親ヘッダーIDとステータスを取得する
pub async fn find_item_owner_status(pool: &PgPool, item_id: i64) -> Result<Option<(i64, String)>> {
    let row: Option<(i64, String)> = sqlx::query_as(
        r#"
        SELECT er.id, er.status
        FROM t_expense_request_item it
        JOIN t_expense_request er ON er.id = it.expense_request_id
        WHERE it.id = $1
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 経費申請一覧（社員名JOIN済み・明細件数付き、最大100件）
///
/// `own_employee_id`が`Some`の場合はその社員の申請のみに絞る
/// （一般社員向け。`None`は全件＝管理者 / can_view_all_expenses 権限者向け）
pub async fn list_expense_rows(pool: &PgPool, own_employee_id: Option<i64>) -> Result<Vec<ExpenseRow>> {
    let rows = sqlx::query_as::<_, ExpenseRow>(
        r#"
        SELECT ex.id, ex.status, ex.total_amount, ex.created_at,
               e.last_name || e.first_name AS employee_name,
               COUNT(it.id) AS item_count
        FROM t_expense_request ex
        JOIN m_employee e ON e.id = ex.employee_id
        LEFT JOIN t_expense_request_item it ON it.expense_request_id = ex.id
        WHERE $1::bigint IS NULL OR ex.employee_id = $1
        GROUP BY ex.id, e.last_name, e.first_name
        ORDER BY ex.created_at DESC LIMIT 100
        "#
    )
    .bind(own_employee_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── 明細 ──

/// 経費申請ヘッダーに紐づく明細一覧（表示順）
pub async fn list_items(pool: &PgPool, expense_request_id: i64) -> Result<Vec<ExpenseRequestItem>> {
    let rows = sqlx::query_as::<_, ExpenseRequestItem>(
        r#"
        SELECT id, expense_request_id, expense_date, category, description, amount,
               (receipt_image IS NOT NULL) AS has_receipt, receipt_mime, display_order
        FROM t_expense_request_item
        WHERE expense_request_id = $1
        ORDER BY display_order, id
        "#
    )
    .bind(expense_request_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 明細1件を取得する（親ヘッダーIDの確認等に使う）
pub async fn find_item_by_id(pool: &PgPool, item_id: i64) -> Result<Option<ExpenseRequestItem>> {
    let row = sqlx::query_as::<_, ExpenseRequestItem>(
        r#"
        SELECT id, expense_request_id, expense_date, category, description, amount,
               (receipt_image IS NOT NULL) AS has_receipt, receipt_mime, display_order
        FROM t_expense_request_item
        WHERE id = $1
        "#
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 明細を追加する（親ヘッダーがPENDINGの場合のみ）。作成されたidを返す
pub async fn add_item(pool: &PgPool, expense_request_id: i64, item: &ExpenseRequestItemForm) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let owner_status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM t_expense_request WHERE id = $1"
    )
    .bind(expense_request_id)
    .fetch_optional(&mut *tx)
    .await?;

    if owner_status.as_deref() != Some("PENDING") {
        anyhow::bail!("この申請は明細を追加できる状態ではありません");
    }

    let next_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(display_order), -1) + 1 FROM t_expense_request_item WHERE expense_request_id = $1"
    )
    .bind(expense_request_id)
    .fetch_one(&mut *tx)
    .await?;

    let item_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_expense_request_item
            (expense_request_id, expense_date, category, description, amount, display_order)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#
    )
    .bind(expense_request_id)
    .bind(item.expense_date)
    .bind(&item.category)
    .bind(&item.description)
    .bind(item.amount)
    .bind(next_order)
    .fetch_one(&mut *tx)
    .await?;

    recompute_total_tx(&mut tx, expense_request_id).await?;
    tx.commit().await?;

    Ok(item_id)
}

/// 明細を更新する（親ヘッダーがPENDINGの場合のみ。更新件数を返す）
pub async fn update_item(pool: &PgPool, item_id: i64, item: &ExpenseRequestItemForm) -> Result<u64> {
    let mut tx = pool.begin().await?;

    let expense_request_id: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT it.expense_request_id
        FROM t_expense_request_item it
        JOIN t_expense_request er ON er.id = it.expense_request_id
        WHERE it.id = $1 AND er.status = 'PENDING'
        "#
    )
    .bind(item_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(expense_request_id) = expense_request_id else {
        return Ok(0);
    };

    let result = sqlx::query(
        r#"
        UPDATE t_expense_request_item
        SET expense_date = $2, category = $3, description = $4, amount = $5, updated_at = NOW()
        WHERE id = $1
        "#
    )
    .bind(item_id)
    .bind(item.expense_date)
    .bind(&item.category)
    .bind(&item.description)
    .bind(item.amount)
    .execute(&mut *tx)
    .await?;

    recompute_total_tx(&mut tx, expense_request_id).await?;
    tx.commit().await?;

    Ok(result.rows_affected())
}

/// 明細を削除する（親ヘッダーがPENDINGの場合のみ。更新件数を返す）
pub async fn delete_item(pool: &PgPool, item_id: i64) -> Result<u64> {
    let mut tx = pool.begin().await?;

    let expense_request_id: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT it.expense_request_id
        FROM t_expense_request_item it
        JOIN t_expense_request er ON er.id = it.expense_request_id
        WHERE it.id = $1 AND er.status = 'PENDING'
        "#
    )
    .bind(item_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(expense_request_id) = expense_request_id else {
        return Ok(0);
    };

    let result = sqlx::query("DELETE FROM t_expense_request_item WHERE id = $1")
        .bind(item_id)
        .execute(&mut *tx)
        .await?;

    recompute_total_tx(&mut tx, expense_request_id).await?;
    tx.commit().await?;

    Ok(result.rows_affected())
}

/// ヘッダーのtotal_amountを明細合計から再計算する
async fn recompute_total_tx(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, expense_request_id: i64) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE t_expense_request
        SET total_amount = COALESCE(
            (SELECT SUM(amount) FROM t_expense_request_item WHERE expense_request_id = $1), 0
        ), updated_at = NOW()
        WHERE id = $1
        "#
    )
    .bind(expense_request_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ── 領収書画像 ──

/// 明細に領収書画像を保存する（親ヘッダーが PENDING/DRAFT のときのみ）
pub async fn save_receipt_image(pool: &PgPool, item_id: i64, image: &[u8], mime: &str) -> Result<()> {
    let owner = find_item_owner_status(pool, item_id).await?;
    let Some((_header_id, status)) = owner else {
        anyhow::bail!("明細が見つかりません");
    };
    if !crate::domain::services::document_workflow::can_edit_content(&status) {
        anyhow::bail!("申請中の経費のみ領収書を添付・変更できます");
    }

    let result = sqlx::query(
        "UPDATE t_expense_request_item SET receipt_image = $2, receipt_mime = $3, updated_at = NOW() WHERE id = $1"
    )
    .bind(item_id)
    .bind(image)
    .bind(mime)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        anyhow::bail!("明細が見つかりません");
    }
    Ok(())
}

/// 明細の領収書画像を取得する
pub async fn find_receipt_image(pool: &PgPool, item_id: i64) -> Result<Option<(Vec<u8>, String)>> {
    let row: Option<(Option<Vec<u8>>, Option<String>)> = sqlx::query_as(
        "SELECT receipt_image, receipt_mime FROM t_expense_request_item WHERE id = $1"
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|(img, mime)| match (img, mime) {
        (Some(img), Some(mime)) => Some((img, mime)),
        _ => None,
    }))
}

// ── 補助 ──

/// 社員のフルネーム（姓 名）を取得する
pub async fn find_employee_full_name(pool: &PgPool, employee_id: i64) -> Result<String> {
    let name: String = sqlx::query_scalar(
        "SELECT last_name || ' ' || first_name FROM m_employee WHERE id = $1"
    )
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    Ok(name)
}

/// 経費科目コードから表示名を取得する
pub async fn find_category_display_name(pool: &PgPool, code: &str) -> Result<Option<String>> {
    let name: Option<String> = sqlx::query_scalar(
        "SELECT name FROM m_expense_category WHERE code = $1"
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(name)
}

/// 社員選択肢一覧
pub async fn list_employee_options(pool: &PgPool) -> Result<Vec<EmployeeOption>> {
    let rows = sqlx::query_as::<_, EmployeeOption>(
        "SELECT id, last_name || first_name AS display_name FROM m_employee ORDER BY last_name, first_name"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 経費科目選択肢一覧（有効なもののみ）
pub async fn list_category_options(pool: &PgPool) -> Result<Vec<ExpenseCategoryOption>> {
    let rows = sqlx::query_as::<_, ExpenseCategoryOption>(
        "SELECT code, name FROM m_expense_category WHERE is_active = TRUE ORDER BY sort_order, id"
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── 旧SSR用（未ルーティング・到達不能。互換のため残す） ──

/// 経費申請ヘッダー登録（旧SSRフォームから。ステータスはDRAFT固定、明細0件で作成）。作成されたidを返す
pub async fn insert_draft(pool: &PgPool, form: &ExpenseRequestForm) -> Result<i64> {
    let mut tx = pool.begin().await?;

    let header_id: i64 = sqlx::query_scalar(
        "INSERT INTO t_expense_request (employee_id, status, total_amount) VALUES ($1, 'DRAFT', $2) RETURNING id"
    )
    .bind(form.employee_id)
    .bind(form.amount)
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO t_expense_request_item
            (expense_request_id, expense_date, category, description, amount, display_order)
        VALUES ($1, $2, $3, $4, $5, 0)
        "#
    )
    .bind(header_id)
    .bind(form.expense_date)
    .bind(&form.category)
    .bind(form.description.as_deref().unwrap_or(""))
    .bind(form.amount)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(header_id)
}

/// 経費申請を提出済み(SUBMITTED)にする（DRAFTのもののみ）
pub async fn submit(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_expense_request SET status = 'SUBMITTED', updated_at = NOW() WHERE id = $1 AND status = 'DRAFT'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 経費申請を承認する（SUBMITTEDのもののみ）
pub async fn approve(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_expense_request SET status = 'APPROVED', approved_at = NOW(), updated_at = NOW() WHERE id = $1 AND status = 'SUBMITTED'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 経費申請を差戻す（SUBMITTEDのもののみ）
pub async fn reject(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE t_expense_request SET status = 'REJECTED', updated_at = NOW() WHERE id = $1 AND status = 'SUBMITTED'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}
