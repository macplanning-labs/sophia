/// infrastructure/mail_pipeline/phase4_store_notify.rs — Phase4: Google Drive保存・エラー通知
///
/// 責務:
/// 1. `status IN ('IMPORTED','FETCH_FAILED','PARSE_FAILED')` かつ `drive_file_id=''` の行を対象に、
///    原本(添付PDF/Excel、またはEDI APIからダウンロードしたPDF)を `drive_service::upload_document`
///    経由でGoogle Driveへアーカイブする。
///    - IMPORTED: 保存成功で完全に終端。失敗時のみ `status='DRIVE_FAILED'`
///      （DB登録自体は既に成功しているため会計データが失われることはない）。
///    - FETCH_FAILED/PARSE_FAILED: 原本があれば「人間が参照できるように」まず保存する。
///      ステータス自体は変更しない（Phase2/3が既に確定させた失敗状態を尊重する）。
/// 2. `needs_manual_review=TRUE AND review_notified_at IS NULL` の行をまとめて、
///    スケジューラ実行1回につき1通のサマリーメールでADMINへ通知する
///    （`reminder_service::check_overdue_tasks` と同じ「ADMIN宛メール」パターンを再利用）。

use sqlx::PgPool;

use super::ops_message;
use crate::domain::services::email_service::EmailService;

/// Phase4実行結果
#[derive(Debug, Default)]
pub struct Phase4Result {
    pub drive_uploaded: usize,
    pub drive_failed: usize,
    pub alert_rows: usize,
    pub alert_sent: bool,
    pub errors: Vec<String>,
}

impl std::fmt::Display for Phase4Result {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Drive保存{}件成功/{}件失敗, 要確認アラート{}件({}), エラー{}件",
            self.drive_uploaded,
            self.drive_failed,
            self.alert_rows,
            if self.alert_sent { "送信済み" } else { "未送信" },
            self.errors.len()
        )
    }
}

/// Phase4エントリポイント
pub async fn run(pool: &PgPool) -> Phase4Result {
    let mut result = Phase4Result::default();

    store_to_drive(pool, &mut result).await;
    notify_manual_review(pool, &mut result).await;

    tracing::info!("[Phase4] 完了: {result}");
    result
}

// ============================================================
// 1. Google Drive保存
// ============================================================

#[derive(Debug, sqlx::FromRow)]
struct StorableEmail {
    id: i64,
    message_id: String,
    subject: String,
    status: String,
    attachment_filename: String,
    raw_attachment: Option<Vec<u8>>,
    received_at: chrono::DateTime<chrono::Utc>,
    company_name: Option<String>,
    is_client: bool,
}

async fn store_to_drive(pool: &PgPool, result: &mut Phase4Result) {
    let rows: Vec<StorableEmail> = match sqlx::query_as(
        r#"
        SELECT
            e.id, e.message_id, e.subject, e.status, e.attachment_filename,
            e.raw_attachment, e.received_at,
            COALESCE(c.name, p.name) AS company_name,
            (e.client_id IS NOT NULL) AS is_client
        FROM t_received_email e
        LEFT JOIN m_client c ON c.id = e.client_id
        LEFT JOIN m_partner p ON p.partner_id = e.partner_id
        WHERE e.status IN ('IMPORTED', 'FETCH_FAILED', 'PARSE_FAILED')
          AND e.drive_file_id = ''
          AND e.raw_attachment IS NOT NULL
        ORDER BY e.received_at
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase4",
                "原本アーカイブ対象の取得",
                "Drive保存がスキップされる",
                format!("{}", ops_message::format_sqlx(&e)),
            ));
            return;
        }
    };

    for row in &rows {
        let Some(bytes) = row.raw_attachment.as_ref() else { continue };

        let management_type = if row.is_client { "client" } else { "partner" };
        let company_name = row.company_name.clone().unwrap_or_else(|| "不明".to_string());
        let doc_label = super::phase1_watch::classify_subject_label(&row.subject);
        let doc_type = match doc_label.as_str() {
            "ORDER" => "order",
            "INVOICE" => "invoice",
            "REPORT" => "work_report",
            "CONTRACT" => "contract",
            _ => "work_report",
        };
        let doc_label_ja = match doc_type {
            "order" => "注文書",
            "invoice" => "請求書",
            "contract" => "契約書",
            _ => "稼働報告書",
        };
        let year_month = row.received_at.format("%Y%m").to_string();
        let short_id = &row.message_id[..row.message_id.len().min(8)];
        let ext = if row.attachment_filename.to_lowercase().ends_with(".xlsx")
            || row.attachment_filename.to_lowercase().ends_with(".xlsm")
            || row.attachment_filename.to_lowercase().ends_with(".xls")
        {
            "xlsx"
        } else {
            "pdf"
        };
        let filename = format!("{doc_label_ja}_{company_name}_{year_month}_{short_id}.{ext}");

        match crate::infrastructure::drive_service::upload_document(
            management_type, &company_name, doc_type, &filename, bytes, None,
        )
        .await
        {
            Ok((file_id, link)) if !file_id.is_empty() => {
                if let Err(e) = sqlx::query(
                    "UPDATE t_received_email SET drive_file_id = $2, drive_link = $3 WHERE id = $1",
                )
                .bind(row.id)
                .bind(&file_id)
                .bind(&link)
                .execute(pool)
                .await
                {
                    result.errors.push(ops_message::fail(
                        "メール取込",
                        "Phase4",
                        "Drive情報の記録",
                        "原本アーカイブ済みだが参照情報が記録されない",
                        format!("メールID={} | {}", row.id, ops_message::format_sqlx(&e)),
                    ));
                }
                result.drive_uploaded += 1;
                tracing::info!("[Phase4] Drive保存成功: メールID={}, filename={filename}", row.id);
            }
            Ok(_) => {
                // GOOGLE_DRIVE_ROOT_FOLDER_ID等が未設定で意図的にスキップされたケース。
                // エラーではないため drive_file_id は空のまま次回以降も対象になり得るが実害はない。
                tracing::debug!("[Phase4] Drive未設定のためスキップ: メールID={}", row.id);
            }
            Err(e) => {
                result.drive_failed += 1;
                let err_msg = ops_message::fail(
                    "メール取込",
                    "Phase4",
                    "原本のGoogle Drive保存",
                    if row.status == "IMPORTED" {
                        "業務DB登録済みだが原本アーカイブ未完了"
                    } else {
                        "原本未保存のまま確認待ち"
                    },
                    format!("メールID={} | {}", row.id, e),
                );
                if row.status == "IMPORTED" {
                    if let Err(update_err) = sqlx::query(
                        "UPDATE t_received_email SET status = 'DRIVE_FAILED', error_message = $2 WHERE id = $1",
                    )
                    .bind(row.id)
                    .bind(&err_msg)
                    .execute(pool)
                    .await
                    {
                        result.errors.push(ops_message::fail(
                            "メール取込",
                            "Phase4",
                            "Drive失敗状態の記録",
                            "失敗情報がDB記録されない",
                            format!("メールID={} | {}", row.id, ops_message::format_sqlx(&update_err)),
                        ));
                    }
                }
                result.errors.push(err_msg);
            }
        }
    }
}

// ============================================================
// 2. 要確認アラートの集約送信
// ============================================================

#[derive(Debug, sqlx::FromRow)]
struct ReviewRow {
    id: i64,
    subject: String,
    source_type: String,
    status: String,
    error_message: String,
}

async fn notify_manual_review(pool: &PgPool, result: &mut Phase4Result) {
    let rows: Vec<ReviewRow> = match sqlx::query_as(
        r#"
        SELECT id, subject, source_type, status, error_message
        FROM t_received_email
        WHERE needs_manual_review = TRUE AND review_notified_at IS NULL
        ORDER BY received_at
        "#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase4",
                "要確認行の取得",
                "管理者へのアラート送信がスキップされる",
                format!("{}", ops_message::format_sqlx(&e)),
            ));
            return;
        }
    };

    if rows.is_empty() {
        return;
    }

    result.alert_rows = rows.len();

    // s_userにrole列は存在しない。ADMIN判定は is_staff=TRUE（role.rs::get_role()と同じ基準）。
    let admin_email: Option<(String,)> = match sqlx::query_as(
        "SELECT email FROM s_user WHERE is_staff = TRUE AND email != '' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    {
        Ok(row) => row,
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール取込",
                "Phase4",
                "ADMIN連絡先の取得",
                "要確認アラートが送信できない",
                format!("{}", ops_message::format_sqlx(&e)),
            ));
            return;
        }
    };

    let Some((admin_email,)) = admin_email else {
        result.errors.push(ops_message::fail(
            "メール取込",
            "Phase4",
            "ADMIN連絡先の検索",
            "要確認アラートが送信できない",
            "管理者ユーザーが登録されていない".to_string(),
        ));
        return;
    };

    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let subject = format!(
        "【Sophia】メール自動取込: 要確認が{}件あります ({today})",
        rows.len()
    );
    let row_lines = rows
        .iter()
        .map(|r| format!("  ・[{}/{}] {} — {}", r.source_type, r.status, r.subject, r.error_message))
        .collect::<Vec<_>>()
        .join("\n");
    let body = format!(
        "管理者各位\n\nメール自動取込パイプラインで以下の{}件が自動処理できず、確認が必要です。\n\n{}\n\n\
         ダッシュボードのメール取込一覧から内容を確認し、必要に応じて手動で登録してください。\n",
        rows.len(),
        row_lines,
    );

    // メールが自社SMTP(=IMAPと同じGmail認証情報)経由のため、その認証情報自体が原因で
    // 取込に失敗しているケースでは通知メールも一緒に失敗しうる(同種の弱点)。
    // Chatはこの認証情報と無関係な別チャネルのため併用する(未着手のまま気づけない、という
    // 声への対策)。
    crate::domain::services::chat_notifier::post(&format!(
        "*【Sophia】メール自動取込: 要確認が{}件あります ({today})*\n{row_lines}",
        rows.len()
    ))
    .await;

    let email_svc = EmailService::new(pool.clone());
    match email_svc.send(&admin_email, None, &subject, &body).await {
        Ok(()) => {
            result.alert_sent = true;
            let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
            if let Err(e) = sqlx::query(
                "UPDATE t_received_email SET review_notified_at = NOW() WHERE id = ANY($1)",
            )
            .bind(&ids)
            .execute(pool)
            .await
            {
                result.errors.push(ops_message::fail(
                    "メール取込",
                    "Phase4",
                    "アラート送信済みの記録",
                    "取込パイプラインの状態管理に齟齬が生じる可能性がある",
                    format!("{}", ops_message::format_sqlx(&e)),
                ));
            }
            tracing::info!("[Phase4] 要確認アラート送信完了: {}件 → {admin_email}", rows.len());
        }
        Err(e) => {
            result.errors.push(ops_message::fail(
                "メール送信",
                "SMTP",
                "要確認アラート送信",
                "業務通知が管理者に届いていない可能性",
                format!("{}", e),
            ));
        }
    }
}
