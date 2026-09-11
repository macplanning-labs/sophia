/// infrastructure/mail_pipeline/circuit_breaker.rs — IMAP認証失敗サーキットブレイカー
///
/// 誤った認証情報での連続IMAPアクセスによるGoogle側の再ブロックを防ぐため、
/// AUTHENTICATIONFAILEDが LOCK_THRESHOLD 回連続したらIMAPアクセスを自動ロックする。
/// ロック解除は管理者の手動操作のみ（一定時間経過での自動解除は行わない —
/// 原因調査前に自動再試行してまた失敗を積み増すことを避けるため）。
///
/// 状態は s_mail_sync_checkpoint（mailbox列がPK）に同居させる。専用テーブルを
/// 新設しない理由: チェックポイントも本来「このメールボックスに対する同期の状態」で
/// あり、同じ主キー(mailbox)に対する追加属性として扱う方が素直なため。

use sqlx::PgPool;

/// この回数連続でIMAP認証に失敗したら自動ロックする
pub const LOCK_THRESHOLD: i32 = 3;

/// エラーメッセージがIMAP認証失敗（AUTHENTICATIONFAILED）由来かどうかを判定する。
///
/// `imap`クレートはエラーを構造化せずサーバー応答文字列をそのまま保持するため、
/// 文字列一致で判定する。ネットワーク瞬断やTLSエラーなど認証以外の失敗では
/// ブレイカーを作動させたくない（過剰にロックして正常運用を止めないため）。
pub fn is_auth_failure(error_message: &str) -> bool {
    error_message.to_uppercase().contains("AUTHENTICATIONFAILED")
}

/// ロック中であればロック理由を返す（ロックされていなければ None）
///
/// DB読取失敗時は ERROR を出しつつ None を返す（可用性優先＝過ロックしない）。
/// ロック判定不能のためサイレント検知・運用ログで気づけるようにする。
pub async fn locked_reason(pool: &PgPool, mailbox: &str) -> Option<String> {
    let row: Result<Option<(Option<String>,)>, _> = sqlx::query_as(
        "SELECT lock_reason FROM s_mail_sync_checkpoint WHERE mailbox = $1 AND locked_at IS NOT NULL",
    )
    .bind(mailbox)
    .fetch_optional(pool)
    .await;

    match row {
        Ok(Some((reason,))) => reason,
        Ok(None) => None,
        Err(e) => {
            tracing::error!(
                "[メール取込/CircuitBreaker] 処理=IMAPロック状態読取 結果=失敗 影響=ロック判定不能（ロックなし扱い） | {}",
                e
            );
            None
        }
    }
}

/// 認証失敗を記録し、更新後の連続失敗回数を返す（レコードが無ければ作成する）
pub async fn record_failure(pool: &PgPool, mailbox: &str) -> i32 {
    let result: Result<(i32,), _> = sqlx::query_as(
        r#"
        INSERT INTO s_mail_sync_checkpoint (mailbox, last_processed_at, consecutive_failures, updated_at)
        VALUES ($1, NOW() - INTERVAL '30 days', 1, NOW())
        ON CONFLICT (mailbox) DO UPDATE SET
            consecutive_failures = s_mail_sync_checkpoint.consecutive_failures + 1,
            updated_at = NOW()
        RETURNING consecutive_failures
        "#,
    )
    .bind(mailbox)
    .fetch_one(pool)
    .await;

    match result {
        Ok((count,)) => count,
        Err(e) => {
            tracing::error!("[CircuitBreaker] 失敗カウント更新エラー: {e}");
            0
        }
    }
}

/// 認証成功時に連続失敗カウントをリセットする
pub async fn record_success(pool: &PgPool, mailbox: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE s_mail_sync_checkpoint SET consecutive_failures = 0 WHERE mailbox = $1",
    )
    .bind(mailbox)
    .execute(pool)
    .await
    {
        tracing::error!(
            "[メール取込/CircuitBreaker] 処理=認証成功カウントリセット 結果=失敗 影響=失敗カウントが残る可能性 | {}",
            e
        );
    }
}

/// IMAPアクセスをロックする
pub async fn lock(pool: &PgPool, mailbox: &str, reason: &str) {
    let result = sqlx::query(
        "UPDATE s_mail_sync_checkpoint SET locked_at = NOW(), lock_reason = $2 WHERE mailbox = $1",
    )
    .bind(mailbox)
    .bind(reason)
    .execute(pool)
    .await;

    match result {
        Ok(_) => tracing::warn!("[CircuitBreaker] IMAPアクセスをロックしました: mailbox={mailbox}"),
        Err(e) => tracing::error!("[CircuitBreaker] ロック処理エラー: {e}"),
    }
}

/// 管理者によるロック解除
pub async fn unlock(pool: &PgPool, mailbox: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE s_mail_sync_checkpoint SET locked_at = NULL, lock_reason = NULL, consecutive_failures = 0 WHERE mailbox = $1",
    )
    .bind(mailbox)
    .execute(pool)
    .await?;
    tracing::info!("[CircuitBreaker] IMAPロックを手動解除しました: mailbox={mailbox}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_authenticationfailed_case_insensitively() {
        assert!(is_auth_failure("IMAPログインエラー: NO [AUTHENTICATIONFAILED] Invalid credentials (Failure)"));
        assert!(is_auth_failure("no [authenticationfailed]"));
        assert!(!is_auth_failure("IMAP接続エラー: connection refused"));
    }
}
