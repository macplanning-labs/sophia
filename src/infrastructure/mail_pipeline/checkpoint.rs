/// infrastructure/mail_pipeline/checkpoint.rs — Phase1 チェックポイント管理
///
/// 「未読/既読」に依存せず、s_mail_sync_checkpoint.last_processed_at を基点に
/// 差分取得を行うためのコアロジック。
///
/// - 取りこぼし対策として、実際のIMAP検索では last_processed_at からさらに
///   SAFETY_MARGIN_DAYS 分さかのぼった日付を使う（IMAP SINCEは日付粒度のため）。
///   重複は t_received_email.message_id の UNIQUE 制約で自然に排除される。
/// - チェックポイント更新には「バッチ実行開始時刻」を使う（完了時刻ではない）。
///   実行中に新着メールが届いても、次回の安全マージンで確実に拾えるようにするため。

use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

/// IMAP SINCE 検索でさかのぼる安全マージン（日数）
///
/// last_processed_at ちょうどからではなく、この日数分余分に再走査する。
/// 重複は message_id の UNIQUE 制約 + ON CONFLICT DO NOTHING で吸収されるため、
/// 再走査コストより「取りこぼし」を防ぐことを優先する。
const SAFETY_MARGIN_DAYS: i64 = 3;

/// 初回実行時（チェックポイント未設定）にさかのぼる日数
const INITIAL_LOOKBACK_DAYS: i64 = 30;

/// チェックポイントを取得する。未設定の場合は INITIAL_LOOKBACK_DAYS 前を返す。
pub async fn get_last_processed_at(pool: &PgPool, mailbox: &str) -> DateTime<Utc> {
    let row: Option<(DateTime<Utc>,)> = sqlx::query_as(
        "SELECT last_processed_at FROM s_mail_sync_checkpoint WHERE mailbox = $1",
    )
    .bind(mailbox)
    .fetch_optional(pool)
    .await
    .unwrap_or(None);

    match row {
        Some((last_processed_at,)) => last_processed_at,
        None => {
            let fallback = Utc::now() - Duration::days(INITIAL_LOOKBACK_DAYS);
            tracing::info!(
                "[チェックポイント] {mailbox} の記録なし。初回実行として{INITIAL_LOOKBACK_DAYS}日前から走査: {fallback}"
            );
            fallback
        }
    }
}

/// IMAP SINCE 検索に使う日付（安全マージンを引いたもの）を算出する
pub fn search_since_date(last_processed_at: DateTime<Utc>) -> DateTime<Utc> {
    last_processed_at - Duration::days(SAFETY_MARGIN_DAYS)
}

/// チェックポイントを更新する（UPSERT）
///
/// 呼び出し側は「バッチ実行を開始した時刻」を渡すこと（完了時刻ではない）。
pub async fn update_checkpoint(
    pool: &PgPool,
    mailbox: &str,
    run_started_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO s_mail_sync_checkpoint (mailbox, last_processed_at, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (mailbox)
        DO UPDATE SET last_processed_at = EXCLUDED.last_processed_at, updated_at = NOW()
        "#,
    )
    .bind(mailbox)
    .bind(run_started_at)
    .execute(pool)
    .await?;

    tracing::info!("[チェックポイント] {mailbox} を更新: last_processed_at={run_started_at}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_since_subtracts_safety_margin() {
        let checkpoint = DateTime::parse_from_rfc3339("2026-07-10T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let since = search_since_date(checkpoint);
        assert_eq!(since, checkpoint - Duration::days(SAFETY_MARGIN_DAYS));
    }
}
