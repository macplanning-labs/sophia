/// infrastructure/repositories/issued_document_repo.rs — 発行した書類(送った瞬間の PDF)の記録(DEMO-000214)
///
/// 表: `t_issued_document`(記録)と `t_issued_document_content`(PDF の本体)。マイグレーション 074。
/// - 記録だけを読む関数(`latest_meta`・`list_pending`)と、本体を読む関数(`load_content`)を分ける。
///   一覧・検索・保存の判断で、大きな本体を読まないため(設計書 5.2)。
/// - 行は更新・削除しない。更新するのは Drive への保存の状態の列だけ(DB のトリガーでも拒む)。

use anyhow::Result;
use chrono::NaiveDate;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// 新しく残す書類
pub struct NewIssuedDocument<'a> {
    pub doc_type: &'a str,
    pub doc_ref: &'a str,
    pub party_kind: &'a str,
    pub party_name: &'a str,
    pub target_month: Option<NaiveDate>,
    pub amount: Option<i64>,
    pub file_name: &'a str,
    pub content: &'a [u8],
}

/// 記録(本体は含まない)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IssuedDocumentMeta {
    pub id: i64,
    pub doc_type: String,
    pub doc_ref: String,
    pub version: i32,
    pub party_kind: String,
    pub party_name: String,
    pub file_name: String,
    pub sha256: String,
    pub store_status: String,
    pub store_attempts: i32,
}

const META_COLUMNS: &str =
    "id, doc_type, doc_ref, version, party_kind, party_name, file_name, sha256, store_status, store_attempts";

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// その書類の最新の版の記録
pub async fn latest_meta(pool: &PgPool, doc_type: &str, doc_ref: &str) -> Result<Option<IssuedDocumentMeta>> {
    let sql = format!(
        "SELECT {META_COLUMNS} FROM t_issued_document WHERE doc_type = $1 AND doc_ref = $2 ORDER BY version DESC LIMIT 1"
    );
    Ok(sqlx::query_as::<_, IssuedDocumentMeta>(&sql)
        .bind(doc_type)
        .bind(doc_ref)
        .fetch_optional(pool)
        .await?)
}

/// 新しい版として残す(版の番号は、既存の最大+1)。記録と本体を同じトランザクションで入れる。
/// 戻り値は (id, version)。
pub async fn insert_new_version(pool: &PgPool, d: &NewIssuedDocument<'_>) -> Result<(i64, i32)> {
    let sha = sha256_hex(d.content);
    let size = i32::try_from(d.content.len()).map_err(|_| anyhow::anyhow!("PDF が大きすぎます({} バイト)", d.content.len()))?;
    let mut tx = pool.begin().await?;
    // 同じ書類の版の採番が重ならないように、書類ごとに排他する(トランザクションの終わりで外れる)
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(format!("issued_document:{}:{}", d.doc_type, d.doc_ref))
        .execute(&mut *tx)
        .await?;
    let next: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM t_issued_document WHERE doc_type = $1 AND doc_ref = $2",
    )
    .bind(d.doc_type)
    .bind(d.doc_ref)
    .fetch_one(&mut *tx)
    .await?;
    let id: i64 = sqlx::query_scalar(
        r#"INSERT INTO t_issued_document
             (doc_type, doc_ref, version, party_kind, party_name, target_month, amount, file_name, sha256, size_bytes)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
           RETURNING id"#,
    )
    .bind(d.doc_type)
    .bind(d.doc_ref)
    .bind(next)
    .bind(d.party_kind)
    .bind(d.party_name)
    .bind(d.target_month)
    .bind(d.amount)
    .bind(d.file_name)
    .bind(&sha)
    .bind(size)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO t_issued_document_content (issued_document_id, content) VALUES ($1, $2)")
        .bind(id)
        .bind(d.content)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((id, next))
}

/// 本体を読む。記録の照合値と一致しなければエラー(壊れた本体を相手に出さない)
pub async fn load_content(pool: &PgPool, id: i64) -> Result<Vec<u8>> {
    let row: Option<(Vec<u8>, String)> = sqlx::query_as(
        r#"SELECT c.content, d.sha256
             FROM t_issued_document_content c
             JOIN t_issued_document d ON d.id = c.issued_document_id
            WHERE c.issued_document_id = $1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let (content, sha) = row.ok_or_else(|| anyhow::anyhow!("発行した書類の本体がありません(id={id})"))?;
    if sha256_hex(&content) != sha {
        anyhow::bail!("発行した書類の本体の照合値が一致しません(id={id})");
    }
    Ok(content)
}

/// Drive へまだ保存していない記録(古い順)
pub async fn list_pending(pool: &PgPool, limit: i64) -> Result<Vec<IssuedDocumentMeta>> {
    let sql = format!(
        "SELECT {META_COLUMNS} FROM t_issued_document WHERE store_status = 'pending' ORDER BY id LIMIT $1"
    );
    Ok(sqlx::query_as::<_, IssuedDocumentMeta>(&sql).bind(limit).fetch_all(pool).await?)
}

/// Drive へ保存できた
pub async fn mark_stored(pool: &PgPool, id: i64, file_id: &str, file_name: &str, url: Option<&str>) -> Result<()> {
    sqlx::query(
        r#"UPDATE t_issued_document
              SET store_status = 'stored', store_file_id = $2, store_file_name = $3, store_url = $4,
                  stored_at = now(), store_last_error = NULL
            WHERE id = $1"#,
    )
    .bind(id)
    .bind(file_id)
    .bind(file_name)
    .bind(url)
    .execute(pool)
    .await?;
    Ok(())
}

/// Drive への保存に失敗した。失敗の回数(更新後)を返す
pub async fn mark_attempt_failed(pool: &PgPool, id: i64, error: &str) -> Result<i32> {
    let short: String = error.chars().take(500).collect();
    let attempts: i32 = sqlx::query_scalar(
        r#"UPDATE t_issued_document
              SET store_attempts = store_attempts + 1, store_last_error = $2
            WHERE id = $1
        RETURNING store_attempts"#,
    )
    .bind(id)
    .bind(short)
    .fetch_one(pool)
    .await?;
    Ok(attempts)
}

/// 画面向け: Drive への保存待ちの件数
pub async fn pending_count(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM t_issued_document WHERE store_status = 'pending'")
        .fetch_one(pool)
        .await?)
}

/// 毎朝の点検向け: 指定の時間より長く、Drive に保存できていない書類の件数(保存の失敗が続いている目安)
pub async fn pending_older_than_hours(pool: &PgPool, hours: i32) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_issued_document WHERE store_status = 'pending' AND sent_at < now() - make_interval(hours => $1)",
    )
    .bind(hours)
    .fetch_one(pool)
    .await?)
}

/// 画面向け: 最後に Drive へ保存したもの(ファイル名と日時)
pub async fn last_stored(pool: &PgPool) -> Result<Option<(String, chrono::DateTime<chrono::Utc>)>> {
    Ok(sqlx::query_as(
        "SELECT store_file_name, stored_at FROM t_issued_document WHERE store_status = 'stored' AND stored_at IS NOT NULL ORDER BY stored_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_is_lower_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn meta_columns_do_not_include_content() {
        // 一覧・検索で本体を読まないこと(設計書 5.2)
        assert!(!META_COLUMNS.contains("content"));
    }
}
