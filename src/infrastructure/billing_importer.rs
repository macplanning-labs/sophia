/// infrastructure/billing_importer.rs — 請求明細の取込で使う部品
///
/// 以前は取引先の EDI から指定月の請求書一覧を取って取り込んでいたが、取引先の EDI へは
/// 通知メールが届いたときだけ接続する方針のため、一覧からの取込は削除した（DEMO-000148）。

use sqlx::PgPool;

/// m_engineer からエンジニア名で検索、未登録なら自動登録
pub(crate) async fn find_or_create_engineer(pool: &PgPool, name: &str) -> Result<i64, String> {
    // まず名前で検索
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_engineer WHERE name = $1"
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    if let Some(id) = id {
        return Ok(id);
    }

    // 部分一致で検索（姓のみ）
    let family_name = name.split_whitespace().next().unwrap_or(name);
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_engineer WHERE name LIKE $1 LIMIT 1"
    )
    .bind(format!("{}%", family_name))
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    if let Some(id) = id {
        tracing::info!("[BillingImporter] エンジニア部分一致: '{}' → id={}", name, id);
        return Ok(id);
    }

    // 未登録なら自動登録（affiliation_type='EMPLOYEE'）
    let new_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO m_engineer (name, affiliation_type, is_active)
        VALUES ($1, 'EMPLOYEE', true)
        RETURNING id
        "#
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("エンジニア自動登録失敗: {}", e))?;

    tracing::info!("[BillingImporter] エンジニア自動登録: '{}' → id={}", name, new_id);
    Ok(new_id)
}
