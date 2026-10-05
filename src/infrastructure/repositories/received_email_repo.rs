/// infrastructure/repositories/received_email_repo.rs — 受信メール(t_received_email) CRUD
///
/// GASのWebhook（稼働報告メール受信通知）と、受信メール一覧・取込画面で使用する。
/// t_mail_scan_log（ダッシュボードのメールチェック一覧用）とは別テーブルなので mail_repo.rs とは分離している。

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
    let domain = from_email.split('@').nth(1).unwrap_or("").to_lowercase();
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
        WHERE LOWER(known_domains.split_part) = $1
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
    let mut rows = query.fetch_all(pool).await?;
    // 過去の不具合で生のMIMEのまま保存された本文は、表示時に復号して読める形にする
    for row in &mut rows {
        row.body_text = crate::infrastructure::mail_pipeline::imap_util::repair_raw_mime_body(&row.body_text);
    }
    Ok(rows)
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

// ── 取引先 EDI の通知メール（メール取込 Phase2。DEMO-000148） ──

/// 未処理の取引先 EDI の通知メール1通
#[derive(Debug, Clone)]
pub struct EdiNoticeRow {
    pub id: i64,
    pub client_id: i64,
    pub subject: String,
    pub body_text: String,
    pub retry_count: i32,
}

/// 未処理の EDI 通知メール。人の確認待ち（needs_manual_review）・再試行の上限に達したもの・
/// 再試行の時刻前のものは含めない（失敗が続いても、ログインを試み続けない）。
pub async fn load_pending_edi_notices(
    pool: &PgPool,
    max_retry: i32,
) -> Result<Vec<EdiNoticeRow>, sqlx::Error> {
    let rows: Vec<(i64, i64, String, String, i32)> = sqlx::query_as(
        r#"
        SELECT id, client_id, subject, body_text, retry_count FROM t_received_email
        WHERE source_type = 'EDI_API'
          AND status IN ('NEW', 'FETCH_FAILED')
          AND client_id IS NOT NULL
          AND message_id NOT LIKE 'edi-order:%'
          AND message_id NOT LIKE 'edi-invoice:%'
          AND needs_manual_review = FALSE
          AND retry_count < $1
          AND (next_retry_at IS NULL OR next_retry_at <= NOW())
        ORDER BY received_at
        "#,
    )
    .bind(max_retry)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, client_id, subject, body_text, retry_count)| EdiNoticeRow {
            id,
            client_id,
            subject,
            body_text,
            retry_count,
        })
        .collect())
}

/// 通知メールを処理済みにする（通知メール自体に取り込む中身は無い）。note に何をしたかを残す。
pub async fn settle_edi_notice(pool: &PgPool, id: i64, note: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE t_received_email SET status = 'SKIPPED', processed_at = NOW(), error_message = $2 WHERE id = $1",
    )
    .bind(id)
    .bind(note)
    .execute(pool)
    .await?;
    Ok(())
}

/// 通知メールを人の確認待ちにする（自動では二度と取りに行かない。管理者への集約通知の対象になる）
pub async fn mark_edi_notice_needs_review(pool: &PgPool, id: i64, reason: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE t_received_email
        SET status = 'FETCH_FAILED', needs_manual_review = TRUE, error_message = $2
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(format!("[Phase2] {reason}"))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn message_id_exists(pool: &PgPool, message_id: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM t_received_email WHERE message_id = $1)")
        .bind(message_id)
        .fetch_one(pool)
        .await
}

/// 受注として登録済みか（取引先 + 先方の注文番号）
pub async fn received_order_exists(pool: &PgPool, client_id: i64, client_order_number: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_received_order WHERE client_id = $1 AND client_order_number = $2)",
    )
    .bind(client_id)
    .bind(client_order_number)
    .fetch_one(pool)
    .await
}

/// 請求書に先方の請求書番号が紐づけ済みか
pub async fn billing_invoice_linked(pool: &PgPool, client_id: i64, edi_invoice_no: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM t_billing_invoice WHERE client_id = $1 AND edi_invoice_no = $2)",
    )
    .bind(client_id)
    .bind(edi_invoice_no)
    .fetch_one(pool)
    .await
}

/// EDI から取得した書類1件を、取得済み（FETCHED）の行として保存する。Phase3 が parsed_data をそのまま登録に使う。
#[allow(clippy::too_many_arguments)]
pub async fn insert_fetched_edi_doc(
    pool: &PgPool,
    message_id: &str,
    from_email: &str,
    from_name: &str,
    subject: &str,
    client_id: i64,
    attachment_filename: &str,
    parsed_data: &serde_json::Value,
    pdf: Option<&[u8]>,
) -> Result<bool, sqlx::Error> {
    let r = sqlx::query(
        r#"
        INSERT INTO t_received_email (
            message_id, from_email, from_name, subject, received_at,
            client_id, status, source_type, attachment_filename, parsed_data, raw_attachment
        ) VALUES ($1, $2, $3, $4, NOW(), $5, 'FETCHED', 'EDI_API', $6, $7, $8)
        ON CONFLICT (message_id) DO NOTHING
        "#,
    )
    .bind(message_id)
    .bind(from_email)
    .bind(from_name)
    .bind(subject)
    .bind(client_id)
    .bind(attachment_filename)
    .bind(parsed_data)
    .bind(pdf)
    .execute(pool)
    .await?;
    Ok(r.rows_affected() > 0)
}
