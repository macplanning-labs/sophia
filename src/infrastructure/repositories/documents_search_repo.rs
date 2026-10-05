/// infrastructure/repositories/documents_search_repo.rs — 帳票横断検索
///
/// 受注書・発注書・請求書・支払通知を横断検索する管理者専用API。
/// `%` と `_` はアプリ側でエスケープし、SQLには ESCAPE 句で指示する。

use anyhow::Result;
use serde::Serialize;
use sqlx::PgPool;
use utoipa::ToSchema;

/// 帳票検索の応答型
#[derive(Debug, Clone, Serialize, ::sqlx::FromRow, ToSchema)]
pub struct DocumentSearchResult {
    /// 帳票種別: "received_order", "order", "invoice", "notice"
    pub r#type: String,
    /// ID (受注書 ID、発注書 ID、請求書番号、支払通知 ID)
    pub id: String,
    /// 番号表示 (受注書番号、発注書番号、請求書番号、支払通知 ID)
    pub number: String,
    /// タイトル (案件名、案件名、件名、空)
    pub title: String,
    /// 取引先名 (クライアント名、パートナー名、クライアント名、パートナー名)
    pub counterparty: String,
    /// 状態
    pub status: String,
    /// 対象月 ("YYYY-MM" format or null)
    pub month: Option<String>,
    /// 詳細URL
    pub url: String,
    /// 並べ替え用の作成日時(応答には出さない)
    #[serde(skip)]
    #[schema(ignore)]
    pub sort_at: chrono::DateTime<chrono::Utc>,
}

/// 検索用 `%` エスケープ関数
/// `%` / `_` / `\` を `\` でエスケープする
pub fn escape_ilike_pattern(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 受注書を検索（前方・部分一致）
async fn search_received_orders(
    pool: &PgPool,
    escaped_query: &str,
    limit: i32,
) -> Result<Vec<DocumentSearchResult>> {
    let pattern = format!("%{}%", escaped_query);
    let rows = sqlx::query_as::<_, DocumentSearchResult>(
        r#"
        SELECT
            'received_order' AS "type",
            ro.id::TEXT AS id,
            ro.received_order_no AS number,
            ro.project_name AS title,
            c.name AS counterparty,
            ro.status,
            TO_CHAR(ro.target_month, 'YYYY-MM') AS month,
            CONCAT('/received-orders/', ro.id) AS url,
            ro.created_at::timestamptz AS sort_at
        FROM t_received_order ro
        JOIN m_client c ON c.id = ro.client_id
        LEFT JOIN m_engineer e ON e.id = ro.engineer_id
        WHERE ro.received_order_no ILIKE $1 ESCAPE '\'
           OR c.name ILIKE $1 ESCAPE '\'
           OR (e.id IS NOT NULL AND e.name ILIKE $1 ESCAPE '\')
        ORDER BY ro.created_at DESC
        LIMIT $2
        "#
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 発注書を検索（前方・部分一致）
async fn search_purchase_orders(
    pool: &PgPool,
    escaped_query: &str,
    limit: i32,
) -> Result<Vec<DocumentSearchResult>> {
    let pattern = format!("%{}%", escaped_query);
    let rows = sqlx::query_as::<_, DocumentSearchResult>(
        r#"
        SELECT
            'order' AS "type",
            po.order_id AS id,
            po.order_id AS number,
            '' AS title,
            p.name AS counterparty,
            po.status,
            NULL::TEXT AS month,
            CONCAT('/orders/', po.order_id) AS url,
            po.created_at::timestamptz AS sort_at
        FROM t_purchase_order po
        JOIN m_partner p ON p.partner_id = po.partner_id
        LEFT JOIN m_engineer e ON e.id = po.engineer_id
        WHERE po.order_id ILIKE $1 ESCAPE '\'
           OR p.name ILIKE $1 ESCAPE '\'
           OR (e.id IS NOT NULL AND e.name ILIKE $1 ESCAPE '\')
        ORDER BY po.created_at DESC
        LIMIT $2
        "#
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 請求書を検索（前方・部分一致）
async fn search_billing_invoices(
    pool: &PgPool,
    escaped_query: &str,
    limit: i32,
) -> Result<Vec<DocumentSearchResult>> {
    let pattern = format!("%{}%", escaped_query);
    let rows = sqlx::query_as::<_, DocumentSearchResult>(
        r#"
        SELECT
            'invoice' AS "type",
            bi.invoice_no AS id,
            bi.invoice_no AS number,
            bi.subject AS title,
            c.name AS counterparty,
            bi.status,
            TO_CHAR(bi.target_month, 'YYYY-MM') AS month,
            CONCAT('/invoices/', bi.id) AS url,
            bi.created_at::timestamptz AS sort_at
        FROM t_billing_invoice bi
        JOIN m_client c ON c.id = bi.client_id
        WHERE bi.invoice_no ILIKE $1 ESCAPE '\'
           OR c.name ILIKE $1 ESCAPE '\'
           OR bi.subject ILIKE $1 ESCAPE '\'
        ORDER BY bi.created_at DESC
        LIMIT $2
        "#
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 支払通知書を検索（前方・部分一致）
async fn search_payment_notices(
    pool: &PgPool,
    escaped_query: &str,
    limit: i32,
) -> Result<Vec<DocumentSearchResult>> {
    let pattern = format!("%{}%", escaped_query);
    let rows = sqlx::query_as::<_, DocumentSearchResult>(
        r#"
        SELECT
            'notice' AS "type",
            pn.notice_id AS id,
            pn.notice_id AS number,
            '' AS title,
            p.name AS counterparty,
            COALESCE(pn.approval_status, '') AS status,
            TO_CHAR(pn.target_month, 'YYYY-MM') AS month,
            CONCAT('/notices/', pn.notice_id) AS url,
            pn.created_at::timestamptz AS sort_at
        FROM t_payment_notice pn
        JOIN m_partner p ON p.partner_id = pn.partner_id
        WHERE pn.notice_id ILIKE $1 ESCAPE '\'
           OR p.name ILIKE $1 ESCAPE '\'
        ORDER BY pn.created_at DESC
        LIMIT $2
        "#
    )
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 新しい順(作成日時の降順)に並べ、合計 limit 件に切る
pub fn sort_and_truncate(mut results: Vec<DocumentSearchResult>, limit: i32) -> Vec<DocumentSearchResult> {
    results.sort_by(|a, b| b.sort_at.cmp(&a.sort_at));
    results.truncate(limit.max(0) as usize);
    results
}

/// 複数の帳票種を横断検索
pub async fn search_documents(
    pool: &PgPool,
    query: &str,
    types: &[&str],
    limit: i32,
) -> Result<Vec<DocumentSearchResult>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("Search query cannot be empty"));
    }

    let escaped = escape_ilike_pattern(trimmed);
    let mut results = Vec::new();
    let per_type_limit = limit;

    // 種別ごとに検索（最大 limit 件）
    for r#type in types {
        match *r#type {
            "received_order" => {
                let rows = search_received_orders(pool, &escaped, per_type_limit).await?;
                results.extend(rows);
            }
            "order" => {
                let rows = search_purchase_orders(pool, &escaped, per_type_limit).await?;
                results.extend(rows);
            }
            "invoice" => {
                let rows = search_billing_invoices(pool, &escaped, per_type_limit).await?;
                results.extend(rows);
            }
            "notice" => {
                let rows = search_payment_notices(pool, &escaped, per_type_limit).await?;
                results.extend(rows);
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown document type: {}", r#type));
            }
        }
    }

    Ok(sort_and_truncate(results, limit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_ilike_pattern() {
        assert_eq!(escape_ilike_pattern("test"), "test");
        assert_eq!(escape_ilike_pattern("test%"), "test\\%");
        assert_eq!(escape_ilike_pattern("test_"), "test\\_");
        assert_eq!(escape_ilike_pattern("test\\"), "test\\\\");
        assert_eq!(escape_ilike_pattern("test%_\\"), "test\\%\\_\\\\");
        assert_eq!(escape_ilike_pattern("日本語"), "日本語");
    }

    fn item(number: &str, minutes_ago: i64) -> DocumentSearchResult {
        DocumentSearchResult {
            r#type: "invoice".into(),
            id: number.into(),
            number: number.into(),
            title: String::new(),
            counterparty: String::new(),
            status: String::new(),
            month: None,
            url: format!("/invoices/{number}"),
            sort_at: chrono::Utc::now() - chrono::Duration::minutes(minutes_ago),
        }
    }

    #[test]
    fn newest_first_and_cut_to_limit() {
        let out = sort_and_truncate(vec![item("B", 30), item("A", 10), item("C", 50), item("D", 20)], 3);
        let numbers: Vec<&str> = out.iter().map(|r| r.number.as_str()).collect();
        assert_eq!(numbers, vec!["A", "D", "B"], "新しい順で3件");
    }

    #[test]
    fn sorting_does_not_depend_on_url_text() {
        // URL の文字列順では Z が先だが、作成日時が新しい A が先に来る
        let mut z = item("Z", 100);
        z.url = "/zzz".into();
        let mut a = item("A", 1);
        a.url = "/aaa".into();
        let out = sort_and_truncate(vec![z, a], 10);
        assert_eq!(out[0].number, "A");
    }
}
