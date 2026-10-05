/// infrastructure/db.rs — データベース接続
///
/// PostgreSQL接続プールの生成。
/// DATABASE_URL 環境変数から接続先を取得する。

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool() -> anyhow::Result<PgPool> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("DATABASE_URL must be set"))?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    tracing::info!("✅ Database connected");
    Ok(pool)
}
