/// infrastructure/billing_importer.rs — EDI-OASIS 請求書取込ロジック
///
/// ## 処理フロー
/// 1. EDI-OASIS API で指定月の請求書一覧を取得
/// 2. 各請求書の詳細 JSON を取得（invoiceJson 内の Go map[] 形式）
/// 3. details 配列をパースして各エンジニアの明細を抽出
/// 4. m_engineer 名前照合（未登録なら自動登録 affiliation_type='EMPLOYEE'）
/// 5. t_billing に UPSERT
/// 6. t_billing_invoice に UPSERT

use chrono::NaiveDate;
use sqlx::PgPool;
use tracing;

use crate::infrastructure::edi_oasis_client::{EdiOasisClient, EdiOasisError, InvoiceDetail, InvoiceDetailItem};

/// 取込結果
#[derive(Debug, Default)]
pub struct BillingImportResult {
    pub invoices_processed: usize,
    pub billings_imported: usize,
    pub billings_skipped: usize,
    pub errors: Vec<String>,
    /// 今回新規/更新登録された請求明細（呼び出し側で「何が新規追加されたか」を通知するのに使う）
    pub new_billings: Vec<String>,
}

impl std::fmt::Display for BillingImportResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "請求取込: 請求書{}件, 明細取込{}件, スキップ{}件, エラー{}件",
            self.invoices_processed, self.billings_imported,
            self.billings_skipped, self.errors.len()
        )
    }
}

/// 指定年月の請求書を EDI-OASIS から一括取込
pub async fn import_billings_for_month(
    pool: &PgPool,
    client: &mut EdiOasisClient,
    year: i32,
    month: i32,
) -> BillingImportResult {
    let mut result = BillingImportResult::default();

    // 1. ログイン
    if let Err(e) = client.login().await {
        result.errors.push(format!("EDI-OASISログイン失敗: {}", e));
        return result;
    }

    // 2. 請求書一覧を取得
    let invoices = match client.fetch_invoices(year, month).await {
        Ok(list) => list,
        Err(e) => {
            result.errors.push(format!("請求書一覧取得失敗: {}", e));
            return result;
        }
    };

    tracing::info!("[BillingImporter] {}年{}月: 請求書{}件取得", year, month, invoices.len());

    // 3. 各請求書の詳細を取得して取込
    for inv_summary in &invoices {
        result.invoices_processed += 1;

        // セッション切れ(Unauthorized)なら1回だけ再ログインしてリトライする
        // （Phase2の`with_retry!`マクロと同じ方針）
        let detail = match client.fetch_invoice_detail(inv_summary.id).await {
            Ok(d) => d,
            Err(EdiOasisError::Unauthorized(_)) => {
                tracing::warn!(
                    "[BillingImporter] OASISセッション切れを検知、再ログインして再試行します (invoice_id={})",
                    inv_summary.id
                );
                let retried = match client.login().await {
                    Ok(()) => client.fetch_invoice_detail(inv_summary.id).await,
                    Err(login_err) => Err(login_err),
                };
                match retried {
                    Ok(d) => d,
                    Err(e) => {
                        result.errors.push(format!(
                            "請求書詳細取得失敗 (id={}): {}", inv_summary.id, e
                        ));
                        continue;
                    }
                }
            }
            Err(e) => {
                result.errors.push(format!(
                    "請求書詳細取得失敗 (id={}): {}", inv_summary.id, e
                ));
                continue;
            }
        };

        // 年月からtarget_monthを生成
        let target_month = match NaiveDate::from_ymd_opt(
            detail.year.unwrap_or(year),
            detail.month.unwrap_or(month) as u32,
            1,
        ) {
            Some(d) => d,
            None => {
                result.errors.push(format!(
                    "無効な年月: {:?}/{:?} (invoice_id={})",
                    detail.year, detail.month, inv_summary.id
                ));
                continue;
            }
        };

        // 各明細を t_billing に投入
        for item in &detail.items {
            match import_single_billing(pool, &detail, item, target_month).await {
                Ok(Some(label)) => {
                    result.billings_imported += 1;
                    result.new_billings.push(label);
                }
                Ok(None) => result.billings_skipped += 1,
                Err(e) => result.errors.push(e),
            }
        }

        // t_billing_invoice に UPSERT
        if let Err(e) = upsert_billing_invoice(pool, &detail, inv_summary.id, target_month).await {
            result.errors.push(e);
        }
    }

    tracing::info!("[BillingImporter] {}", result);
    result
}

/// 1件の請求明細を t_billing に UPSERT。
/// 戻り値: 新規追加なら `Some(識別ラベル)`、既存行の更新なら `None`
async fn import_single_billing(
    pool: &PgPool,
    _detail: &InvoiceDetail,
    item: &InvoiceDetailItem,
    target_month: NaiveDate,
) -> Result<Option<String>, String> {
    // エンジニア名で m_engineer を照合
    let engineer_id = match find_or_create_engineer(pool, &item.item_name).await {
        Ok(id) => id,
        Err(e) => return Err(format!("エンジニア照合失敗 ({}): {}", item.item_name, e)),
    };

    // クライアント: EDI-OASIS は常にイービジネス社経由
    // EB のクライアントIDを取得（名前の部分一致で検索）
    let client_id = find_eb_client_id(pool).await
        .map_err(|e| format!("クライアント検索失敗: {}", e))?;

    // 精算タイプ推定
    let settlement_type = if item.item_min_hours > 0.0 && item.item_max_hours > 0.0 {
        "RANGE"
    } else {
        "FIXED"
    };

    // 調整金計算
    let adjustment = item.item_amount as i32 - item.item_basic_amount as i32;

    // UPSERT（`xmax = 0` は「今回INSERTで新規追加された行か」の判定に使う定番のPostgresイディオム。
    // ON CONFLICT DO UPDATEは常に行を書き込むため、rows_affected()だけでは新規/更新を区別できない）
    let (is_new,): (bool,) = sqlx::query_as(
        r#"
        INSERT INTO t_billing (
            client_id, target_month, engineer_id, base_rate, settlement_type,
            lower_limit_hours, upper_limit_hours, deduction_rate, overtime_rate,
            effort, actual_hours, adjustment, amount, source
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, 'EDI')
        ON CONFLICT (client_id, target_month, engineer_id) DO UPDATE SET
            base_rate = EXCLUDED.base_rate,
            settlement_type = EXCLUDED.settlement_type,
            lower_limit_hours = EXCLUDED.lower_limit_hours,
            upper_limit_hours = EXCLUDED.upper_limit_hours,
            deduction_rate = EXCLUDED.deduction_rate,
            overtime_rate = EXCLUDED.overtime_rate,
            effort = EXCLUDED.effort,
            actual_hours = EXCLUDED.actual_hours,
            adjustment = EXCLUDED.adjustment,
            amount = EXCLUDED.amount,
            updated_at = NOW()
        RETURNING (xmax = 0) AS is_new
        "#
    )
    .bind(client_id)
    .bind(target_month)
    .bind(engineer_id)
    .bind(item.item_basic_amount as i32)
    .bind(settlement_type)
    .bind(item.item_min_hours)
    .bind(item.item_max_hours)
    .bind(item.item_minus_per_hour as i32)
    .bind(item.item_plus_per_hour as i32)
    .bind(item.item_rate)
    .bind(item.item_total_hours)
    .bind(adjustment)
    .bind(item.item_amount as i32)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("t_billing UPSERT失敗: {}", e))?;

    tracing::info!(
        "[BillingImporter] t_billing UPSERT: engineer={}, month={}, amount={}, 新規={}",
        item.item_name, target_month, item.item_amount, is_new,
    );

    if is_new {
        Ok(Some(format!(
            "{}（{}, ¥{}）",
            item.item_name,
            target_month.format("%Y年%m月"),
            item.item_amount
        )))
    } else {
        Ok(None)
    }
}

/// t_billing_invoice に UPSERT + t_billing_invoice_item に明細行を作成
async fn upsert_billing_invoice(
    pool: &PgPool,
    detail: &InvoiceDetail,
    edi_id: i64,
    target_month: NaiveDate,
) -> Result<(), String> {
    let client_id = find_eb_client_id(pool).await
        .map_err(|e| format!("クライアント検索失敗: {}", e))?;

    let invoice_no = detail.invoice_no.clone()
        .unwrap_or_else(|| format!("EDI-{}", edi_id));

    // work_start / work_end をパース
    let work_start = detail.work_start.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
            .or_else(|| NaiveDate::parse_from_str(s, "%Y/%m/%d").ok()))
        .unwrap_or(target_month);

    let work_end = detail.work_end.as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
            .or_else(|| NaiveDate::parse_from_str(s, "%Y/%m/%d").ok()))
        .unwrap_or_else(|| {
            // 月末を計算
            if target_month.month() == 12 {
                NaiveDate::from_ymd_opt(target_month.year() + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(target_month.year(), target_month.month() + 1, 1)
            }
            .and_then(|d| d.pred_opt())
            .unwrap_or(target_month)
        });

    use chrono::Datelike;

    // 明細合計
    let subtotal: i32 = detail.items.iter().map(|i| i.item_amount as i32).sum();
    let tax_amount: i32 = detail.items.iter().map(|i| i.item_tax_amount as i32).sum();
    let total = subtotal + tax_amount;

    // 件名を自動生成
    let subject = format!("{}年{:02}月分 請求書", target_month.year(), target_month.month());

    // t_billing_invoice の (client_id, target_month) UNIQUE制約は 008_invoice_self_issue.sql で
    // 廃止済み（同月同クライアントで複数の自社発行請求書(source='SELF')を許可するため）。
    // そのためON CONFLICTは使えず、EDI取込分(source='EDI')に限定して検索→INSERT/UPDATEを手動分岐する。
    let existing_id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM t_billing_invoice WHERE client_id = $1 AND target_month = $2 AND source = 'EDI' LIMIT 1"
    )
    .bind(client_id)
    .bind(target_month)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("t_billing_invoice検索エラー: {}", e))?;

    let invoice_id: i64 = if let Some(id) = existing_id {
        sqlx::query(
            r#"
            UPDATE t_billing_invoice SET
                subtotal = $2,
                tax_amount = $3,
                total = $4,
                subject = CASE WHEN subject = '' THEN $5 ELSE subject END,
                edi_id = $6,
                edi_invoice_no = $7,
                updated_at = NOW()
            WHERE id = $1
            "#
        )
        .bind(id)
        .bind(subtotal)
        .bind(tax_amount)
        .bind(total)
        .bind(&subject)
        .bind(edi_id)
        .bind(&invoice_no)
        .execute(pool)
        .await
        .map_err(|e| format!("t_billing_invoice UPDATE失敗: {}", e))?;
        id
    } else {
        sqlx::query_scalar(
            r#"
            INSERT INTO t_billing_invoice (
                invoice_no, client_id, target_month, work_start, work_end,
                issue_date, subject, subtotal, tax_amount, total,
                edi_id, edi_invoice_no, source, status
            ) VALUES ($1, $2, $3, $4, $5, CURRENT_DATE, $6, $7, $8, $9, $10, $11, 'EDI', 'CONFIRMED')
            RETURNING id
            "#
        )
        .bind(&invoice_no)
        .bind(client_id)
        .bind(target_month)
        .bind(work_start)
        .bind(work_end)
        .bind(&subject)
        .bind(subtotal)
        .bind(tax_amount)
        .bind(total)
        .bind(edi_id)
        .bind(&invoice_no)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("t_billing_invoice INSERT失敗: {}", e))?
    };

    tracing::info!(
        "[BillingImporter] t_billing_invoice UPSERT: id={}, invoice_no={}, month={}, total={}",
        invoice_id, invoice_no, target_month, total,
    );

    // ── 受注への紐づけ・ステータス反映 ──
    // 1つのEDI請求書(t_billing_invoice)は、クライアント＋対象月が同じでも複数エンジニア
    // （＝複数のt_received_order）の合算であることが多い。以前は client_id+target_month
    // だけで LIMIT 1 検索していたため、複数受注のうち1件にしか紐づかない不具合があった。
    // 明細(detail.items)のエンジニアごとに対応する受注を個別に特定し、それぞれのステータスを
    // INVOICED（請求書）へ進める。ヘッダ側のreceived_order_idは1列しか持てないため、
    // 参照表示用として最初に見つかった1件を代表として保持する。
    let mut linked_order_ids: Vec<i64> = Vec::new();
    for item in &detail.items {
        let engineer_id = match find_or_create_engineer(pool, &item.item_name).await {
            Ok(id) => id,
            Err(_) => continue,
        };
        let ro_id: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM t_received_order WHERE client_id = $1 AND target_month = $2 AND engineer_id = $3"
        )
        .bind(client_id)
        .bind(target_month)
        .bind(engineer_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

        let Some(ro_id) = ro_id else { continue };
        linked_order_ids.push(ro_id);

        // REGISTERED/REPORT_RECEIVED/REPORT_SENT の間にある受注のみ進める
        // （既にINVOICED/PAID/CANCELLEDの場合は変更しない）
        let _ = sqlx::query(
            "UPDATE t_received_order SET status = 'INVOICED', updated_at = NOW() \
             WHERE id = $1 AND status IN ('REGISTERED', 'REPORT_RECEIVED', 'REPORT_SENT')"
        )
        .bind(ro_id)
        .execute(pool)
        .await;
    }

    if let Some(&first_ro_id) = linked_order_ids.first() {
        let _ = sqlx::query(
            "UPDATE t_billing_invoice SET received_order_id = $1 WHERE id = $2 AND received_order_id IS NULL"
        )
        .bind(first_ro_id)
        .bind(invoice_id)
        .execute(pool)
        .await;
    }
    tracing::info!(
        "[BillingImporter] 受注紐づけ・ステータス反映: invoice_id={} → {}件 (ro_ids={:?})",
        invoice_id, linked_order_ids.len(), linked_order_ids,
    );

    // ── 明細行を t_billing_invoice_item に作成 ──
    // 冪等性確保: 既存の明細を削除してから再挿入
    sqlx::query("DELETE FROM t_billing_invoice_item WHERE invoice_id = $1")
        .bind(invoice_id)
        .execute(pool)
        .await
        .map_err(|e| format!("t_billing_invoice_item 削除失敗: {}", e))?;

    let mut item_errors: Vec<String> = Vec::new();
    for item in &detail.items {
        // エンジニアID取得
        let engineer_id = find_or_create_engineer(pool, &item.item_name).await
            .unwrap_or(0);

        // 精算タイプ推定
        let settlement_type = if item.item_min_hours > 0.0 && item.item_max_hours > 0.0 {
            "RANGE"
        } else {
            "FIXED"
        };

        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO t_billing_invoice_item (
                invoice_id, engineer_id, description,
                quantity, unit_price, amount,
                settlement_type, lower_limit, upper_limit,
                deduction_rate, overtime_rate
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#
        )
        .bind(invoice_id)
        .bind(engineer_id)
        .bind(&item.item_name)
        .bind(item.item_rate)
        .bind(item.item_basic_amount as i32)
        .bind(item.item_amount as i32)
        .bind(settlement_type)
        .bind(item.item_min_hours)
        .bind(item.item_max_hours)
        .bind(item.item_minus_per_hour as i32)
        .bind(item.item_plus_per_hour as i32)
        .execute(pool)
        .await {
            let error_msg = format!("t_billing_invoice_item INSERT失敗(engineer={}): {}", item.item_name, e);
            tracing::error!("[BillingImporter] {}", error_msg);
            item_errors.push(error_msg);
        }
    }

    tracing::info!(
        "[BillingImporter] t_billing_invoice_item: invoice_id={}, items={}件",
        invoice_id, detail.items.len(),
    );

    if !item_errors.is_empty() {
        let combined_error = format!("請求明細保存エラー（処理=請求明細保存 影響=明細欠落の可能性）: {}", item_errors.join("; "));
        return Err(combined_error);
    }

    Ok(())
}

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

/// イービジネス社のクライアントIDを取得
async fn find_eb_client_id(pool: &PgPool) -> Result<i64, String> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM m_client WHERE name LIKE '%イー・ビジネス%' OR name LIKE '%イービジネス%' LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    id.ok_or_else(|| "イービジネス社のクライアントが見つかりません".to_string())
}
