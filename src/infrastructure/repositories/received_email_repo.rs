/// infrastructure/repositories/received_email_repo.rs — 受信メール(t_received_email) CRUD
///
/// GASのWebhook（稼働報告メール受信通知）と、受信メール一覧・取込画面で使用する。
/// t_mail_scan_log（EDI-OASIS取込用）とは別テーブルなので mail_repo.rs とは分離している。

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::received_email::ReceivedEmail;

/// Webhookで受信したメールを記録する（重複はmessage_idで無視）
#[allow(clippy::too_many_arguments)]
pub async fn insert_received_email(
    pool: &PgPool,
    message_id: &str,
    from_email: &str,
    from_name: &str,
    subject: &str,
    body_text: &str,
    attachment_filename: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO t_received_email (message_id, from_email, from_name, subject, received_at, body_text, attachment_filename)
        VALUES ($1, $2, $3, $4, NOW(), $5, $6)
        ON CONFLICT (message_id) DO NOTHING
        "#
    )
    .bind(message_id)
    .bind(from_email)
    .bind(from_name)
    .bind(subject)
    .bind(body_text)
    .bind(attachment_filename)
    .execute(pool)
    .await?;
    Ok(())
}

/// 送信者ドメインが既知の取引先ドメインと一致するか判定
///
/// m_client.email / m_partner.email / その他の設定済みメールアドレスのドメイン部分と
/// from_email のドメイン部分を比較する（Phase1 と一覧で同じロジックを使用するため）。
pub async fn sender_matches_known_domain(pool: &PgPool, from_email: &str) -> Result<bool> {
    let domain = from_email.split('@').nth(1).unwrap_or("");
    if domain.is_empty() {
        return Ok(false);
    }

    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM (
            SELECT DISTINCT split_part(email, '@', 2) FROM m_client WHERE email <> ''
            UNION SELECT DISTINCT split_part(edi_notification_email, '@', 2) FROM m_client WHERE edi_notification_email <> ''
            UNION SELECT DISTINCT split_part(work_report_email, '@', 2) FROM m_client WHERE work_report_email <> ''
            UNION SELECT DISTINCT split_part(invoice_email, '@', 2) FROM m_client WHERE invoice_email <> ''
            UNION SELECT DISTINCT split_part(email, '@', 2) FROM m_partner WHERE email <> ''
        ) AS known_domains
        WHERE known_domains.split_part = $1
        "#
    )
    .bind(domain)
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

/// 受信メール一覧（新しい順、最大100件）
///
/// - status: 空文字ならフィルタなし
/// - needs_review: Some(true)なら「要確認」のみ、Some(false)なら「要確認以外」、Noneならフィルタなし
/// - known_domain_only: trueなら、差出人アドレスのドメインがm_client/m_partnerに登録された
///   いずれかのメールドメインと一致するものだけに絞る（個人宛の迷惑メール・通知等を除外する用途）
pub async fn list_received_emails(
    pool: &PgPool,
    status: &str,
    needs_review: Option<bool>,
    known_domain_only: bool,
) -> Result<Vec<ReceivedEmail>> {
    let mut sql = String::from("SELECT * FROM t_received_email WHERE 1=1");
    let mut bind_idx = 1;
    if !status.is_empty() {
        sql.push_str(&format!(" AND status = ${bind_idx}"));
        bind_idx += 1;
    }
    if needs_review.is_some() {
        sql.push_str(&format!(" AND needs_manual_review = ${bind_idx}"));
    }
    if known_domain_only {
        sql.push_str(
            r#" AND split_part(from_email, '@', 2) IN (
                SELECT DISTINCT split_part(email, '@', 2) FROM m_client WHERE email <> ''
                UNION SELECT DISTINCT split_part(edi_notification_email, '@', 2) FROM m_client WHERE edi_notification_email <> ''
                UNION SELECT DISTINCT split_part(work_report_email, '@', 2) FROM m_client WHERE work_report_email <> ''
                UNION SELECT DISTINCT split_part(invoice_email, '@', 2) FROM m_client WHERE invoice_email <> ''
                UNION SELECT DISTINCT split_part(email, '@', 2) FROM m_partner WHERE email <> ''
            )"#,
        );
    }
    sql.push_str(" ORDER BY received_at DESC LIMIT 100");

    let mut query = sqlx::query_as::<_, ReceivedEmail>(&sql);
    if !status.is_empty() {
        query = query.bind(status);
    }
    if let Some(v) = needs_review {
        query = query.bind(v);
    }
    Ok(query.fetch_all(pool).await?)
}

/// 受信メールを取込済み(IMPORTED)にする（更新件数を返す。対象がNEW以外なら0件）
pub async fn mark_imported(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_received_email SET status = 'IMPORTED', processed_at = NOW() WHERE id = $1 AND status = 'NEW'"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// 「要確認」フラグを解除する（担当者が手動で内容を確認・対応済みにした場合）
pub async fn mark_manually_resolved(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE t_received_email SET needs_manual_review = FALSE WHERE id = $1"
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// needs_manual_review = TRUE の受信メール件数（ダッシュボードバッジ用）
pub async fn count_needs_manual_review(pool: &PgPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_received_email WHERE needs_manual_review = TRUE",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// status = 'FETCH_FAILED' の受信メール件数（ダッシュボードバッジ用）
pub async fn count_fetch_failed(pool: &PgPool) -> Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_received_email WHERE status = 'FETCH_FAILED'",
    )
    .fetch_one(pool)
    .await?;
    Ok(count)
}

/// 受信メールを削除する（不要と判断されたメールをDBから物理削除する）
pub async fn delete_received_email(pool: &PgPool, id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM t_received_email WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}
