/// infrastructure/repositories/tax_rate_repo.rs — 消費税率マスタ(m_tax_rate)参照

use rust_decimal::Decimal;
use sqlx::PgPool;

/// 指定日時点で有効な消費税率をm_tax_rateから取得する。
/// 該当データがない場合は10.00（標準税率）にフォールバックする。
pub async fn find_effective_rate(pool: &PgPool, date: chrono::NaiveDate) -> anyhow::Result<Decimal> {
    let rate: Option<Decimal> = sqlx::query_scalar(
        "SELECT rate FROM m_tax_rate \
         WHERE effective_from <= $1 AND (effective_to IS NULL OR effective_to >= $1) \
         ORDER BY effective_from DESC LIMIT 1"
    )
    .bind(date)
    .fetch_optional(pool)
    .await?;
    Ok(rate.unwrap_or_else(|| Decimal::from(10)))
}
