/// infrastructure/edi_order_importer.rs — EDI注文書取込ロジック
///
/// ## 処理フロー
/// 1. t_received_email から未処理の EDI 注文書メールを取得
/// 2. メール本文から detail_id を抽出
/// 3. EDI-OASIS API で注文詳細 JSON を取得
/// 4. t_received_order に INSERT（重複チェック付き）
/// 5. t_received_email.status を 'IMPORTED' に更新

use chrono::{Datelike, NaiveDate};
use sqlx::PgPool;
use tracing;

use crate::infrastructure::edi_oasis_client::{EdiOasisClient, EdiOasisError, OrderDetail};
use crate::infrastructure::repositories::order_repo;

/// 取込結果
#[derive(Debug, Default)]
pub struct ImportResult {
    pub processed: usize,
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl std::fmt::Display for ImportResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EDI取込: 処理{}件, 取込{}件, スキップ{}件, エラー{}件",
            self.processed, self.imported, self.skipped, self.errors.len()
        )
    }
}

/// 受信メールの最小情報
#[derive(Debug, sqlx::FromRow)]
struct PendingEmail {
    id: i64,
    body_text: String,
    subject: String,
    from_email: String,
}

/// 未処理の EDI 注文書メールを一括処理する（スケジューラ用）
pub async fn process_pending_edi_orders(pool: &PgPool) -> ImportResult {
    let mut result = ImportResult::default();

    // 1. EDI注文書メールを取得（status='NEW' かつ EDI URL 含む）
    let emails = match sqlx::query_as::<_, PendingEmail>(
        r#"
        SELECT id, body_text, subject, from_email
        FROM t_received_email
        WHERE status = 'NEW'
          AND body_text LIKE '%edi.e-business.co.jp%'
          AND (body_text LIKE '%注文%' OR subject LIKE '%注文%')
        ORDER BY received_at
        "#
    )
    .fetch_all(pool)
    .await
    {
        Ok(emails) => emails,
        Err(e) => {
            result.errors.push(format!("DB検索エラー: {e}"));
            return result;
        }
    };

    if emails.is_empty() {
        tracing::debug!("[EDI取込] 未処理のEDI注文書メールなし");
        return result;
    }

    tracing::info!("[EDI取込] 未処理メール {}件を処理開始", emails.len());

    // 2. OASISクライアント初期化 + ログイン
    let mut client = match EdiOasisClient::from_env() {
        Ok(c) => c,
        Err(EdiOasisError::NotConfigured) => {
            tracing::warn!("[EDI取込] EDI_OASIS認証情報が未設定、スキップ");
            return result;
        }
        Err(e) => {
            result.errors.push(format!("OASISクライアント初期化エラー: {e}"));
            return result;
        }
    };

    if let Err(e) = client.login().await {
        result.errors.push(format!("OASISログインエラー: {e}"));
        return result;
    }

    // 3. 各メールを処理
    for email in &emails {
        result.processed += 1;

        match process_single_email(pool, &client, email).await {
            Ok(imported) => {
                if imported {
                    result.imported += 1;
                } else {
                    result.skipped += 1;
                }
            }
            Err(e) => {
                result.errors.push(format!("メールID={}: {e}", email.id));
                tracing::warn!("[EDI取込] メールID={} 処理エラー: {e}", email.id);
            }
        }
    }

    tracing::info!("[EDI取込] {result}");
    result
}

/// 指定メールIDの EDI 取込を実行する（手動実行用）
pub async fn import_from_email(pool: &PgPool, email_id: i64) -> Result<ImportResult, String> {
    let mut result = ImportResult::default();

    // メール取得
    let email = sqlx::query_as::<_, PendingEmail>(
        "SELECT id, body_text, subject, from_email FROM t_received_email WHERE id = $1"
    )
    .bind(email_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB検索エラー: {e}"))?
    .ok_or_else(|| format!("メールID={email_id} が見つかりません"))?;

    // OASISクライアント初期化 + ログイン
    let mut client = EdiOasisClient::from_env()
        .map_err(|e| format!("OASISクライアント初期化エラー: {e}"))?;

    client.login().await
        .map_err(|e| format!("OASISログインエラー: {e}"))?;

    result.processed = 1;

    match process_single_email(pool, &client, &email).await {
        Ok(imported) => {
            if imported {
                result.imported = 1;
            } else {
                result.skipped = 1;
            }
        }
        Err(e) => {
            result.errors.push(e.clone());
            return Err(e);
        }
    }

    Ok(result)
}

/// 1件のメールを処理
///
/// Returns: Ok(true) = 取込成功, Ok(false) = スキップ
async fn process_single_email(
    pool: &PgPool,
    client: &EdiOasisClient,
    email: &PendingEmail,
) -> Result<bool, String> {
    // 1. detail_id 抽出
    let detail_id = match EdiOasisClient::extract_order_detail_id(&email.body_text) {
        Some(id) => id,
        None => {
            tracing::debug!("[EDI取込] メールID={} からURLを抽出できず、スキップ", email.id);
            // EDI URL が含まれているが detail_id が取れない場合もある
            return Ok(false);
        }
    };

    tracing::info!(
        "[EDI取込] メールID={}, detail_id={}, 件名={}",
        email.id, detail_id, email.subject
    );

    // 2. 重複チェック（同じ detail_id で既に取込済みか）
    let already_imported: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM t_received_email
            WHERE status = 'IMPORTED'
              AND body_text LIKE '%detail:' || $1::TEXT || '%'
              AND id != $2
        )
        "#
    )
    .bind(detail_id)
    .bind(email.id)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if already_imported {
        tracing::info!(
            "[EDI取込] detail_id={} は既に取込済み、スキップ",
            detail_id
        );
        // status を SKIPPED に更新
        if let Err(e) = sqlx::query(
            "UPDATE t_received_email SET status = 'SKIPPED', processed_at = NOW() WHERE id = $1"
        )
        .bind(email.id)
        .execute(pool)
        .await { tracing::error!("DB error: {:?}", e); }
        return Ok(false);
    }

    // 3. OASIS API で注文詳細を取得
    let detail = client.fetch_order_detail(detail_id).await
        .map_err(|e| format!("OASIS API エラー: {e}"))?;

    // 4. 注文詳細をログに記録（フィールドマッピング確認用）
    tracing::info!(
        "[EDI取込] OASIS注文詳細: id={:?}, order_no={:?}, project={:?}, year={:?}, month={:?}, worker={:?}, amount={:?}",
        detail.id, detail.order_no, detail.project_name,
        detail.year, detail.month, detail.worker_name, detail.amount,
    );

    // 5. クライアント特定（イービジネス）
    let client_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client WHERE name LIKE '%イー・ビジネス%' OR name LIKE '%イービジネス%' LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let client_id = match client_id {
        Some(id) => id,
        None => {
            return Err("クライアント「イー・ビジネス」が m_client に見つかりません".to_string());
        }
    };

    // 6. 対象月・作業期間を算出
    let (target_month, work_start, work_end) = parse_dates(&detail)?;

    // 7. 重複チェック（同じクライアント + 対象月 + 注文番号）
    let order_no = detail.order_no.as_deref().unwrap_or("");
    if !order_no.is_empty() {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM t_received_order
                WHERE client_id = $1 AND target_month = $2 AND client_order_number = $3
            )
            "#
        )
        .bind(client_id)
        .bind(target_month)
        .bind(order_no)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if exists {
            tracing::info!(
                "[EDI取込] 受注重複: client_id={}, month={}, order_no={}",
                client_id, target_month, order_no
            );
            if let Err(e) = sqlx::query(
                "UPDATE t_received_email SET status = 'SKIPPED', processed_at = NOW(), error_message = '受注重複' WHERE id = $1"
            )
            .bind(email.id)
            .execute(pool)
            .await { tracing::error!("DB error: {:?}", e); }
            return Ok(false);
        }
    }

    // 8. worker_name → m_engineer 検索 → engineer_id 解決
    let engineer_id: Option<i64> = if let Some(ref worker) = detail.worker_name {
        match crate::infrastructure::billing_importer::find_or_create_engineer(pool, worker).await {
            Ok(id) => {
                tracing::info!("[EDI取込] エンジニア解決: '{}' → id={}", worker, id);
                Some(id)
            }
            Err(e) => {
                tracing::warn!("[EDI取込] エンジニア解決失敗: '{}' → {}", worker, e);
                None
            }
        }
    } else {
        None
    };

    // 9. t_received_order に INSERT
    let project_name = detail.project_name.as_deref().unwrap_or("EDI注文");
    let order_date = detail.order_date.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(chrono::Local::now().date_naive());

    let order_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            client_id, engineer_id, target_month, work_start, work_end,
            project_name, status, order_date, client_order_number,
            remarks
        ) VALUES ($1, $2, $3, $4, $5, $6, 'REGISTERED', $7, $8, $9)
        RETURNING id
        "#
    )
    .bind(client_id)
    .bind(engineer_id)
    .bind(target_month)
    .bind(work_start)
    .bind(work_end)
    .bind(project_name)
    .bind(order_date)
    .bind(order_no)
    .bind(format!("EDI-OASIS自動取込 (detail_id={})", detail_id))
    .fetch_one(pool)
    .await
    .map_err(|e| format!("受注INSERT エラー: {e}"))?;

    // 10. 明細作成（OASIS JSONにワーカー情報がある場合）
    if let Some(ref worker) = detail.worker_name {
        let unit_price = detail.unit_price.unwrap_or(0) as i32;
        let amount = detail.amount.unwrap_or(0) as i32;

        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO t_received_order_item (
                order_id, engineer_name, unit_price, man_month,
                settlement_type, base_rate, lower_limit_hours, upper_limit_hours,
                deduction_rate, overtime_rate, effort, mid_month_rule, amount
            ) VALUES ($1, $2, $3, 1.0, 'FIXED', $3, 140, 180, $3, $3, 1.0, 'NONE', $4)
            "#
        )
        .bind(order_id)
        .bind(worker)
        .bind(unit_price)
        .bind(amount)
        .execute(pool)
        .await { tracing::error!("DB error: {:?}", e); }
    }

    match order_repo::try_auto_link_received_order_contract(pool, order_id).await {
        Ok(outcome) => order_repo::log_auto_link_outcome("EDI取込", order_id, &outcome),
        Err(e) => tracing::warn!("[EDI取込] 受注契約自動紐付けエラー(order_id={order_id}): {e}"),
    }

    // 10. メールステータス更新
    if let Err(e) = sqlx::query(
        r#"
        UPDATE t_received_email
        SET status = 'IMPORTED',
            processed_at = NOW(),
            error_message = $2
        WHERE id = $1
        "#
    )
    .bind(email.id)
    .bind(format!("EDI取込成功: received_order_id={}", order_id))
    .execute(pool)
    .await { tracing::error!("DB error: {:?}", e); }

    tracing::info!(
        "[EDI取込] 受注登録完了: order_id={}, client_id={}, month={}, project={}",
        order_id, client_id, target_month, project_name
    );

    Ok(true)
}

/// OASIS注文詳細から日付を算出する
pub(crate) fn parse_dates(detail: &OrderDetail) -> Result<(NaiveDate, NaiveDate, NaiveDate), String> {
    // year + month → 対象月（月初）
    // OrderDetail のフィールドが None の場合、extra（flatten）から取得を試みる
    let year = detail.year.or_else(|| {
        detail.extra.get("year")
            .and_then(|v| v.as_i64().map(|n| n as i32)
                .or_else(|| v.as_str().and_then(|s| s.parse().ok())))
    });
    let month = detail.month.or_else(|| {
        detail.extra.get("month")
            .and_then(|v| v.as_i64().map(|n| n as i32)
                .or_else(|| v.as_str().and_then(|s| s.parse().ok())))
    });
    let work_start_str = detail.work_start.clone().or_else(|| {
        // extra から work_start_date / work_end_date を探す
        for key in &["work_start_date", "workStartDate", "start_date", "startDate"] {
            if let Some(v) = detail.extra.get(*key) {
                if let Some(s) = v.as_str() { return Some(s.to_string()); }
            }
        }
        // header 内の work_start_date も探す
        if let Some(header) = detail.extra.get("header") {
            for key in &["work_start_date", "work_end_date"] {
                if let Some(v) = header.get(key) {
                    if let Some(s) = v.as_str() { return Some(s.to_string()); }
                }
            }
        }
        None
    });

    let target_month = match (year, month) {
        (Some(y), Some(m)) => {
            NaiveDate::from_ymd_opt(y, m as u32, 1)
                .ok_or_else(|| format!("無効な年月: {y}/{m}"))?
        }
        _ => {
            // work_start から推定
            if let Some(ref ws) = work_start_str {
                let d = NaiveDate::parse_from_str(ws, "%Y-%m-%d")
                    .or_else(|_| NaiveDate::parse_from_str(ws, "%Y/%m/%d"))
                    .map_err(|e| format!("work_start パースエラー: {e}"))?;
                NaiveDate::from_ymd_opt(d.year(), d.month(), 1)
                    .ok_or_else(|| "対象月算出エラー".to_string())?
            } else {
                return Err("対象年月が不明です（year/month, work_start いずれもなし）".to_string());
            }
        }
    };

    // 作業期間
    let work_start = detail.work_start.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d")).ok())
        .unwrap_or(target_month);

    let work_end = detail.work_end.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .or_else(|_| NaiveDate::parse_from_str(s, "%Y/%m/%d")).ok())
        .unwrap_or_else(|| {
            // 月末を計算
            let next_month = if target_month.month() == 12 {
                NaiveDate::from_ymd_opt(target_month.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(target_month.year(), target_month.month() + 1, 1)
            };
            next_month
                .map(|d| d.pred_opt().unwrap_or(d))
                .unwrap_or(target_month)
        });

    Ok((target_month, work_start, work_end))
}

/// ダッシュボード（t_mail_scan_log）からの EDI 取込
///
/// ダッシュボードの「受注」ボタンから呼ばれる。
/// mail_id は t_mail_scan_log の ID、detail_id はメール本文から抽出済み。
pub async fn import_from_mail_scan_log(
    pool: &PgPool,
    mail_id: i64,
    detail_id: i64,
) -> Result<ImportResult, String> {
    let mut result = ImportResult::default();
    result.processed = 1;

    // 1. OASISクライアント初期化 + ログイン
    let mut client = EdiOasisClient::from_env()
        .map_err(|e| format!("OASISクライアント初期化エラー: {e}"))?;

    client.login().await
        .map_err(|e| format!("OASISログインエラー: {e}"))?;

    // 2. OASIS API で注文詳細を取得
    let detail = client.fetch_order_detail(detail_id).await
        .map_err(|e| format!("OASIS API エラー: {e}"))?;

    tracing::info!(
        "[EDI取込] OASIS注文詳細: id={:?}, order_no={:?}, project={:?}, worker={:?}, year={:?}, month={:?}, work_start={:?}",
        detail.id, detail.order_no, detail.project_name, detail.worker_name,
        detail.year, detail.month, detail.work_start,
    );
    // extraフィールドの全キーをログ出力（フィールドマッピングのデバッグ用）
    let extra_keys: Vec<&String> = detail.extra.keys().collect();
    tracing::info!("[EDI取込] extra keys: {:?}", extra_keys);
    if !detail.extra.is_empty() {
        tracing::info!("[EDI取込] extra全体: {}", serde_json::to_string_pretty(&detail.extra).unwrap_or_default());
    }

    // 3. クライアント特定（イービジネス）
    let client_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client WHERE name LIKE '%イー・ビジネス%' OR name LIKE '%イービジネス%' LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let client_id = match client_id {
        Some(id) => id,
        None => {
            return Err("クライアント「イー・ビジネス」が m_client に見つかりません".to_string());
        }
    };

    // 4. 日付算出
    let (target_month, work_start, work_end) = parse_dates(&detail)?;

    // 5. 重複チェック
    let order_no = detail.order_no.as_deref().unwrap_or("");
    if !order_no.is_empty() {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM t_received_order
                WHERE client_id = $1 AND target_month = $2 AND client_order_number = $3
            )
            "#
        )
        .bind(client_id)
        .bind(target_month)
        .bind(order_no)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if exists {
            tracing::info!(
                "[EDI取込] 受注重複: client_id={}, month={}, order_no={}",
                client_id, target_month, order_no
            );
            result.skipped = 1;
            return Ok(result);
        }
    }

    // 6. worker_name → m_engineer 検索 → engineer_id 解決
    let engineer_id: Option<i64> = if let Some(ref worker) = detail.worker_name {
        match crate::infrastructure::billing_importer::find_or_create_engineer(pool, worker).await {
            Ok(id) => {
                tracing::info!("[EDI取込] エンジニア解決: '{}' → id={}", worker, id);
                Some(id)
            }
            Err(e) => {
                tracing::warn!("[EDI取込] エンジニア解決失敗: '{}' → {}", worker, e);
                None
            }
        }
    } else {
        None
    };

    // 7. t_received_order に INSERT
    let project_name = detail.project_name.as_deref().unwrap_or("EDI注文");
    let order_date = detail.order_date.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(chrono::Local::now().date_naive());

    let order_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            client_id, engineer_id, target_month, work_start, work_end,
            project_name, status, order_date, client_order_number,
            remarks
        ) VALUES ($1, $2, $3, $4, $5, $6, 'REGISTERED', $7, $8, $9)
        RETURNING id
        "#
    )
    .bind(client_id)
    .bind(engineer_id)
    .bind(target_month)
    .bind(work_start)
    .bind(work_end)
    .bind(project_name)
    .bind(order_date)
    .bind(order_no)
    .bind(format!("EDI-OASIS取込 (detail_id={}, mail_scan_log_id={})", detail_id, mail_id))
    .fetch_one(pool)
    .await
    .map_err(|e| format!("受注INSERT エラー: {e}"))?;

    // 7. 明細作成
    if let Some(ref worker) = detail.worker_name {
        let unit_price = detail.unit_price.unwrap_or(0) as i32;
        let amount = detail.amount.unwrap_or(0) as i32;

        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO t_received_order_item (
                order_id, engineer_name, unit_price, man_month,
                settlement_type, base_rate, lower_limit_hours, upper_limit_hours,
                deduction_rate, overtime_rate, effort, mid_month_rule, amount
            ) VALUES ($1, $2, $3, 1.0, 'FIXED', $3, 140, 180, $3, $3, 1.0, 'NONE', $4)
            "#
        )
        .bind(order_id)
        .bind(worker)
        .bind(unit_price)
        .bind(amount)
        .execute(pool)
        .await { tracing::error!("DB error: {:?}", e); }
    }

    match order_repo::try_auto_link_received_order_contract(pool, order_id).await {
        Ok(outcome) => order_repo::log_auto_link_outcome("EDI取込", order_id, &outcome),
        Err(e) => tracing::warn!("[EDI取込] 受注契約自動紐付けエラー(order_id={order_id}): {e}"),
    }

    tracing::info!(
        "[EDI取込] 受注登録完了: order_id={}, client_id={}, month={}, project={}",
        order_id, client_id, target_month, project_name
    );

    // 8. OASIS 側で注文書を「受領」する
    match client.approve_order(detail_id).await {
        Ok(()) => {
            tracing::info!("[EDI取込] OASIS受領完了: detail_id={}", detail_id);
        }
        Err(e) => {
            // 受領失敗しても受注登録は成功扱い（ログに警告を記録）
            tracing::warn!("[EDI取込] OASIS受領失敗（受注登録は成功）: detail_id={}, error={}", detail_id, e);
        }
    }

    result.imported = 1;
    Ok(result)
}

/// OASIS 注文IDから直接取込+受領する（ダッシュボードのOASIS注文一覧から呼ばれる）
pub async fn import_from_oasis_direct(
    pool: &PgPool,
    order_id: i64,
) -> Result<ImportResult, String> {
    let mut result = ImportResult::default();
    result.processed = 1;

    // 1. OASISクライアント初期化 + ログイン
    let mut client = EdiOasisClient::from_env()
        .map_err(|e| format!("OASISクライアント初期化エラー: {e}"))?;
    client.login().await
        .map_err(|e| format!("OASISログインエラー: {e}"))?;

    // 2. 注文詳細を取得
    let detail = client.fetch_order_detail(order_id).await
        .map_err(|e| format!("OASIS API エラー: {e}"))?;

    tracing::info!(
        "[EDI取込] OASIS注文詳細: id={:?}, order_no={:?}, project={:?}, worker={:?}",
        detail.id, detail.order_no, detail.project_name, detail.worker_name,
    );

    // 3. クライアント特定
    let client_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client WHERE name LIKE '%イー・ビジネス%' OR name LIKE '%イービジネス%' LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let client_id = match client_id {
        Some(id) => id,
        None => return Err("クライアント「イー・ビジネス」が m_client に見つかりません".to_string()),
    };

    // 4. 日付算出
    let (target_month, work_start, work_end) = parse_dates(&detail)?;

    // 5. 重複チェック
    let order_no = detail.order_no.as_deref().unwrap_or("");
    if !order_no.is_empty() {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM t_received_order WHERE client_id = $1 AND target_month = $2 AND client_order_number = $3)"
        )
        .bind(client_id)
        .bind(target_month)
        .bind(order_no)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if exists {
            tracing::info!("[EDI取込] 受注重複: client_id={}, month={}, order_no={}", client_id, target_month, order_no);
            result.skipped = 1;
            return Ok(result);
        }
    }

    // 6. worker_name → m_engineer 検索 → engineer_id 解決
    let engineer_id: Option<i64> = if let Some(ref worker) = detail.worker_name {
        match crate::infrastructure::billing_importer::find_or_create_engineer(pool, worker).await {
            Ok(id) => {
                tracing::info!("[EDI取込] エンジニア解決: '{}' → id={}", worker, id);
                Some(id)
            }
            Err(e) => {
                tracing::warn!("[EDI取込] エンジニア解決失敗: '{}' → {}", worker, e);
                None
            }
        }
    } else {
        None
    };

    // 7. 受注登録
    let project_name = detail.project_name.as_deref().unwrap_or("EDI注文");
    let order_date = detail.order_date.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or(chrono::Local::now().date_naive());

    // received_order_no を自動生成（RO-YYYYMM-NNN）
    let month_str = target_month.format("%Y%m").to_string();
    let existing_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM t_received_order WHERE received_order_no LIKE $1"
    )
    .bind(format!("RO-{}-%", month_str))
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let received_order_no = format!("RO-{}-{:03}", month_str, existing_count + 1);

    let recv_order_id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO t_received_order (
            received_order_no, client_id, engineer_id, target_month, work_start, work_end,
            project_name, status, order_date, client_order_number, remarks
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'REGISTERED', $8, $9, $10)
        RETURNING id
        "#
    )
    .bind(&received_order_no)
    .bind(client_id)
    .bind(engineer_id)
    .bind(target_month)
    .bind(work_start)
    .bind(work_end)
    .bind(project_name)
    .bind(order_date)
    .bind(order_no)
    .bind(format!("EDI-OASIS直接取込 (order_id={})", order_id))
    .fetch_one(pool)
    .await
    .map_err(|e| format!("受注INSERT エラー: {e}"))?;

    // 7. 明細作成
    if let Some(ref worker) = detail.worker_name {
        let unit_price = detail.unit_price.unwrap_or(0) as i32;
        let amount = detail.amount.unwrap_or(0) as i32;
        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO t_received_order_item (
                order_id, engineer_name, unit_price, man_month,
                settlement_type, base_rate, lower_limit_hours, upper_limit_hours,
                deduction_rate, overtime_rate, effort, mid_month_rule, amount
            ) VALUES ($1, $2, $3, 1.0, 'FIXED', $3, 140, 180, $3, $3, 1.0, 'NONE', $4)
            "#
        )
        .bind(recv_order_id)
        .bind(worker)
        .bind(unit_price)
        .bind(amount)
        .execute(pool)
        .await { tracing::error!("DB error: {:?}", e); }
    }

    match order_repo::try_auto_link_received_order_contract(pool, recv_order_id).await {
        Ok(outcome) => order_repo::log_auto_link_outcome("EDI直接取込", recv_order_id, &outcome),
        Err(e) => tracing::warn!("[EDI直接取込] 受注契約自動紐付けエラー(order_id={recv_order_id}): {e}"),
    }

    tracing::info!("[EDI取込] 受注登録完了: order_id={}, project={}", recv_order_id, project_name);

    // 8. 注文書HTMLをダウンロード → Google Drive保存
    match client.download_order_html(order_id).await {
        Ok(html_bytes) => {
            let filename = format!(
                "注文書_株式会社イー・ビジネス_{}.html",
                order_no,
            );
            match crate::infrastructure::drive_service::upload_document(
                "client", "株式会社イー・ビジネス", "order",
                &filename, &html_bytes, Some("text/html"),
            ).await {
                Ok((file_id, link)) if !file_id.is_empty() => {
                    tracing::info!("[EDI取込] Google Drive保存: {} → {}", filename, link);
                }
                Ok(_) => {
                    tracing::info!("[EDI取込] Drive未設定のためスキップ");
                }
                Err(e) => {
                    tracing::error!("[EDI取込] 処理=注文書Drive保存 影響=原本未保管 | {}", e);
                }
            }
        }
        Err(e) => tracing::warn!("[EDI取込] 注文書HTMLダウンロード失敗: {}", e),
    }

    // 9. OASIS受領
    match client.approve_order(order_id).await {
        Ok(()) => tracing::info!("[EDI取込] OASIS受領完了: order_id={}", order_id),
        Err(e) => tracing::warn!("[EDI取込] OASIS受領失敗: order_id={}, error={}", order_id, e),
    }

    result.imported = 1;
    Ok(result)
}
