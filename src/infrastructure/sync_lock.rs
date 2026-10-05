/// infrastructure/sync_lock.rs — 排他制御ユーティリティ
///
/// Django版 core/domain/services/sync_lock.py の移植。
/// PostgreSQL のアドバイザリーロックで外部連携の排他制御を行う。

use anyhow::Result;
use sqlx::PgPool;
use tracing::debug;

/// 排他制御用のロックID（一意であればよい）
const SYNC_LOCK_ID: i64 = 999001;

/// PostgreSQL Advisory Lock を取得して処理を実行し、完了後にロックを解放する。
/// Webhook 同時書込み防止に使用。
pub async fn with_sync_lock<F, T>(pool: &PgPool, func: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    debug!("ロック取得待ち: sync_lock({})", SYNC_LOCK_ID);
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SYNC_LOCK_ID)
        .execute(pool)
        .await?;
    debug!("ロック取得: sync_lock({})", SYNC_LOCK_ID);

    let result = func.await;

    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SYNC_LOCK_ID)
        .execute(pool)
        .await?;
    debug!("ロック解放: sync_lock({})", SYNC_LOCK_ID);

    result
}

/// トランザクション内で Advisory Lock を使用する場合
/// (トランザクション終了時に自動解放される)
pub async fn acquire_tx_lock(pool: &PgPool) -> Result<()> {
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SYNC_LOCK_ID)
        .execute(pool)
        .await?;
    debug!("トランザクションロック取得: sync_lock({})", SYNC_LOCK_ID);
    Ok(())
}
