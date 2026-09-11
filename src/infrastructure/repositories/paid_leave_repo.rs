/// infrastructure/repositories/paid_leave_repo.rs — 年次有給休暇の付与・消化・残高管理
///
/// 労働基準法に基づく年次有給休暇（入社6ヶ月で10日付与、以後1年ごとに増加、
/// 繰越込み2年で時効消滅）を管理する。給与の月次確定処理（`payroll.rs::confirm`）
/// から呼び出され、対象月末時点の付与判定・失効判定・当月消化の反映を行う。

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use sqlx::PgPool;

use crate::domain::models::payroll::PaidLeaveGrant;

/// 正社員（週5日／週30時間以上）の標準付与テーブル。
/// index 0=6ヶ月, 1=1年6ヶ月, 2=2年6ヶ月, ... 6以降=6年6ヶ月以降（20日で頭打ち）
const STATUTORY_GRANT_DAYS: [i64; 7] = [10, 11, 12, 14, 16, 18, 20];

/// 比例付与（パートタイム）テーブル。行=週所定労働日数(1〜4)、列=STATUTORY_GRANT_DAYSと同じ基準日index
const PROPORTIONAL_GRANT_DAYS: [[i64; 7]; 4] = [
    [1, 2, 2, 2, 3, 3, 3],     // 週1日
    [3, 4, 4, 5, 6, 6, 7],     // 週2日
    [5, 6, 6, 8, 9, 10, 11],   // 週3日
    [7, 8, 9, 10, 12, 13, 15], // 週4日
];

/// 付与日数を返す（週所定労働日数が5日以上またはNULLなら正社員テーブル）
fn grant_days_for(weekly_prescribed_days: Option<i16>, anniversary_index: usize) -> Decimal {
    let idx = anniversary_index.min(6);
    let days = match weekly_prescribed_days {
        Some(d) if (1..=4).contains(&d) => PROPORTIONAL_GRANT_DAYS[(d - 1) as usize][idx],
        _ => STATUTORY_GRANT_DAYS[idx],
    };
    Decimal::from(days)
}

/// hire_date を起点に、6ヶ月後・1年6ヶ月後・2年6ヶ月後...と続く付与基準日を列挙する
fn anniversary_dates(hire_date: NaiveDate, up_to: NaiveDate, max_count: usize) -> Vec<(NaiveDate, usize)> {
    let mut result = Vec::new();
    let first = add_months(hire_date, 6);
    for i in 0..max_count {
        let d = add_months(first, (i as u32) * 12);
        if d > up_to {
            break;
        }
        result.push((d, i));
    }
    result
}

fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    let total = date.year() as u32 * 12 + (date.month() - 1) + months;
    let year = (total / 12) as i32;
    let month = total % 12 + 1;
    // 月末日が存在しない場合（うるう年の2/29など）はその月の末日に丸める
    NaiveDate::from_ymd_opt(year, month, date.day())
        .unwrap_or_else(|| last_day_of_month(year, month))
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).unwrap().pred_opt().unwrap()
}

/// 対象月末時点で未付与の基準日があれば t_paid_leave_grant に追加する
async fn grant_if_due(
    pool: &PgPool,
    employee_id: i64,
    hire_date: NaiveDate,
    weekly_prescribed_days: Option<i16>,
    month_end: NaiveDate,
) -> Result<()> {
    for (grant_date, idx) in anniversary_dates(hire_date, month_end, 30) {
        let days = grant_days_for(weekly_prescribed_days, idx);
        let expire_date = add_months(grant_date, 24);
        sqlx::query(
            r#"
            INSERT INTO t_paid_leave_grant (employee_id, grant_date, granted_days, expire_date, remaining_days)
            VALUES ($1, $2, $3, $4, $3)
            ON CONFLICT (employee_id, grant_date) DO NOTHING
            "#,
        )
        .bind(employee_id)
        .bind(grant_date)
        .bind(days)
        .bind(expire_date)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// 対象月末時点で期限切れの付与バッチを時効消滅（残日数0）にする
async fn expire_due_grants(pool: &PgPool, employee_id: i64, month_end: NaiveDate) -> Result<()> {
    sqlx::query(
        "UPDATE t_paid_leave_grant SET remaining_days = 0 \
         WHERE employee_id = $1 AND expire_date < $2 AND remaining_days > 0",
    )
    .bind(employee_id)
    .bind(month_end)
    .execute(pool)
    .await?;
    Ok(())
}

/// 対象月に取得された有給日数を、失効が近い付与バッチから順に(FIFO)消化する
async fn consume_month_usage(pool: &PgPool, employee_id: i64, year_month: NaiveDate) -> Result<Decimal> {
    let used: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(days_used), 0) FROM t_paid_leave_usage WHERE employee_id = $1 AND year_month = $2",
    )
    .bind(employee_id)
    .bind(year_month)
    .fetch_one(pool)
    .await?;

    let mut remaining_to_consume = used;
    if remaining_to_consume > Decimal::ZERO {
        let grants = sqlx::query_as::<_, PaidLeaveGrant>(
            "SELECT * FROM t_paid_leave_grant WHERE employee_id = $1 AND remaining_days > 0 \
             ORDER BY expire_date ASC",
        )
        .bind(employee_id)
        .fetch_all(pool)
        .await?;

        for grant in grants {
            if remaining_to_consume <= Decimal::ZERO {
                break;
            }
            let consume = remaining_to_consume.min(grant.remaining_days);
            sqlx::query("UPDATE t_paid_leave_grant SET remaining_days = remaining_days - $1 WHERE id = $2")
                .bind(consume)
                .bind(grant.id)
                .execute(pool)
                .await?;
            remaining_to_consume -= consume;
        }
    }

    Ok(used)
}

/// 全付与バッチの残日数合計を返す
async fn total_balance(pool: &PgPool, employee_id: i64) -> Result<Decimal> {
    let total: Decimal = sqlx::query_scalar(
        "SELECT COALESCE(SUM(remaining_days), 0) FROM t_paid_leave_grant WHERE employee_id = $1",
    )
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    Ok(total)
}

/// 最も近く失効する付与バッチ（残日数>0のもの）を1件返す（画面表示用の注記に使う）
pub async fn nearest_expiring_grant(pool: &PgPool, employee_id: i64) -> Result<Option<PaidLeaveGrant>> {
    let row = sqlx::query_as::<_, PaidLeaveGrant>(
        "SELECT * FROM t_paid_leave_grant WHERE employee_id = $1 AND remaining_days > 0 \
         ORDER BY expire_date ASC LIMIT 1",
    )
    .bind(employee_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 給与の月次確定時に呼び出す。付与判定 → 失効判定 → 当月消化の反映まで一括処理し、
/// (当月使用日数, 確定時点の残日数) を返す。呼び出し側で t_payroll にスナップショットする。
pub async fn process_month_end(
    pool: &PgPool,
    employee_id: i64,
    hire_date: Option<NaiveDate>,
    weekly_prescribed_days: Option<i16>,
    year_month: NaiveDate,
) -> Result<(Decimal, Decimal)> {
    let Some(hire_date) = hire_date else {
        // 入社日未登録の社員は有給計算の対象外（0/0を返す）
        return Ok((Decimal::ZERO, Decimal::ZERO));
    };

    let month_end = last_day_of_month(year_month.year(), year_month.month());

    grant_if_due(pool, employee_id, hire_date, weekly_prescribed_days, month_end).await?;
    expire_due_grants(pool, employee_id, month_end).await?;
    let used = consume_month_usage(pool, employee_id, year_month).await?;
    let balance = total_balance(pool, employee_id).await?;

    Ok((used, balance))
}

/// t_payroll に有給休暇のスナップショット（当月使用日数・確定時点残日数）を書き込む
pub async fn snapshot_payroll(pool: &PgPool, payroll_id: i64, used: Decimal, balance: Decimal) -> Result<()> {
    sqlx::query("UPDATE t_payroll SET paid_leave_used_days = $1, paid_leave_balance_days = $2 WHERE id = $3")
        .bind(used)
        .bind(balance)
        .bind(payroll_id)
        .execute(pool)
        .await?;
    Ok(())
}
