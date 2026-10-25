//! 口座番号の 3 列(暗号化の変換コマンドが使う)。DEMO-000233 項目4 / S2-G1
//!
//! 通常の画面・帳票の読み書きは、各リポジトリが `data_crypto` を通す(S2-G2)。ここは、既存の平文を
//! `encrypt-bank-accounts` で一度だけ変換するための、列を直接読む・書く関数だけを持つ。

use sqlx::{PgConnection, PgPool};

use crate::domain::value_objects::account_number::{AccountNumber, StoredAccountNumber};
use crate::infrastructure::data_crypto::{self, DataCrypto};

/// 口座番号の読み書きの失敗。復号の失敗を「空」に置き換えない(空の口座番号で帳票が出るのを防ぐ)
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("口座番号のデータベース操作に失敗しました: {0}")]
    Db(#[from] sqlx::Error),
    #[error("口座番号の暗号化・復号に失敗しました: {0}")]
    Crypto(String),
}

/// 口座番号を持つ 3 列
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// 自社の振込先(s_company_info)
    CompanyInfo,
    /// パートナーの請求先口座(m_partner)
    Partner,
    /// 社員の振込先(m_employee_bank_account)
    EmployeeBank,
}

pub const TARGETS: [Target; 3] = [Target::CompanyInfo, Target::Partner, Target::EmployeeBank];

impl Target {
    /// 「表.列」の名前。暗号化の AAD(列の入れ替えを検知する印)にもなるので、変えてはいけない
    pub fn label(self) -> &'static str {
        match self {
            Target::CompanyInfo => "s_company_info.account_number",
            Target::Partner => "m_partner.account_number",
            Target::EmployeeBank => "m_employee_bank_account.account_number",
        }
    }
}

/// 行の主キー(文字列にそろえる)と、口座番号の値
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRow {
    pub key: String,
    pub value: String,
}

/// 3 列とも TEXT に広がっているか(083 が適用済みか)。確認(dry-run)が、マイグレーションを流さずに使う
pub async fn all_text(conn: &mut PgConnection) -> Result<bool, sqlx::Error> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE column_name = 'account_number' AND data_type = 'text' \
         AND table_name IN ('s_company_info', 'm_partner', 'm_employee_bank_account')",
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(n == 3)
}

/// 変換の間、他の書き込みを止める(読み取りは止めない)。同じ表への書き込みと競合するロック。
/// ロックを待つのは 5 秒まで(待ちきれなければ失敗させ、トランザクションを取り消す)
pub async fn lock_table(conn: &mut PgConnection, t: Target) -> Result<(), sqlx::Error> {
    sqlx::query("SET LOCAL lock_timeout = '5s'").execute(&mut *conn).await?;
    match t {
        Target::CompanyInfo => sqlx::query("LOCK TABLE s_company_info IN SHARE ROW EXCLUSIVE MODE").execute(&mut *conn).await?,
        Target::Partner => sqlx::query("LOCK TABLE m_partner IN SHARE ROW EXCLUSIVE MODE").execute(&mut *conn).await?,
        Target::EmployeeBank => sqlx::query("LOCK TABLE m_employee_bank_account IN SHARE ROW EXCLUSIVE MODE").execute(&mut *conn).await?,
    };
    Ok(())
}

/// 全行の(主キー, 口座番号)
pub async fn list_all(conn: &mut PgConnection, t: Target) -> Result<Vec<AccountRow>, sqlx::Error> {
    let rows: Vec<(String, String)> = match t {
        Target::CompanyInfo => {
            sqlx::query_as("SELECT id::TEXT, account_number FROM s_company_info ORDER BY id")
                .fetch_all(&mut *conn)
                .await?
        }
        Target::Partner => {
            sqlx::query_as("SELECT partner_id, account_number FROM m_partner ORDER BY partner_id")
                .fetch_all(&mut *conn)
                .await?
        }
        Target::EmployeeBank => {
            sqlx::query_as("SELECT id::TEXT, account_number FROM m_employee_bank_account ORDER BY id")
                .fetch_all(&mut *conn)
                .await?
        }
    };
    Ok(rows.into_iter().map(|(key, value)| AccountRow { key, value }).collect())
}

/// 1 行の口座番号を書き換える(主キーで指定)
pub async fn update_value(conn: &mut PgConnection, t: Target, key: &str, value: &str) -> Result<u64, sqlx::Error> {
    let done = match t {
        Target::CompanyInfo => {
            sqlx::query("UPDATE s_company_info SET account_number = $1 WHERE id::TEXT = $2")
                .bind(value)
                .bind(key)
                .execute(&mut *conn)
                .await?
        }
        Target::Partner => {
            sqlx::query("UPDATE m_partner SET account_number = $1 WHERE partner_id = $2")
                .bind(value)
                .bind(key)
                .execute(&mut *conn)
                .await?
        }
        Target::EmployeeBank => {
            sqlx::query("UPDATE m_employee_bank_account SET account_number = $1 WHERE id::TEXT = $2")
                .bind(value)
                .bind(key)
                .execute(&mut *conn)
                .await?
        }
    };
    Ok(done.rows_affected())
}

/// 1 行の主キー文字列に対応する行の口座番号を 1 列だけ読む(SQL はこのモジュールだけに置く)。
/// 行が無ければ Ok(None)。NULL は未登録として空を返す。
async fn fetch_stored(conn: &mut PgConnection, t: Target, key: &str) -> Result<Option<Option<String>>, sqlx::Error> {
    match t {
        Target::CompanyInfo => {
            sqlx::query_scalar("SELECT account_number FROM s_company_info WHERE id::TEXT = $1")
                .bind(key)
                .fetch_optional(&mut *conn)
                .await
        }
        Target::Partner => {
            sqlx::query_scalar("SELECT account_number FROM m_partner WHERE partner_id = $1")
                .bind(key)
                .fetch_optional(&mut *conn)
                .await
        }
        Target::EmployeeBank => {
            sqlx::query_scalar("SELECT account_number FROM m_employee_bank_account WHERE id::TEXT = $1")
                .bind(key)
                .fetch_optional(&mut *conn)
                .await
        }
    }
}

/// 1 行の口座番号を読んで復号する(設定を引数で受け取る版。試験・内部用)。
/// 行が無ければ Ok(None)。復号できなければ Err(空で代用しない)。
pub async fn read_account_with(dc: &DataCrypto, conn: &mut PgConnection, t: Target, key: &str) -> Result<Option<AccountNumber>, AccountError> {
    match fetch_stored(conn, t, key).await? {
        None => Ok(None),
        Some(None) => Ok(Some(AccountNumber::from_decrypted(String::new()))),
        // 平文の空文字は未登録として空を返す(REQUIRED でも読める。NULL と同じ扱い)。
        // `enc1:` で始まる壊れた値は、この分岐に入らず下で Err になる
        Some(Some(raw)) if raw.is_empty() => Ok(Some(AccountNumber::from_decrypted(String::new()))),
        Some(Some(raw)) => {
            let stored = StoredAccountNumber::from_db(raw);
            let plain = dc.read_value(t.label(), stored.raw()).map_err(AccountError::Crypto)?;
            Ok(Some(AccountNumber::from_decrypted(plain)))
        }
    }
}

/// 1 行の口座番号を暗号化して書く(設定を引数で受け取る版。試験・内部用)。行が無ければ Ok(0)。
pub async fn write_account_with(dc: &DataCrypto, conn: &mut PgConnection, t: Target, key: &str, value: &AccountNumber) -> Result<u64, AccountError> {
    let stored = dc.write_value(t.label(), value.expose()).map_err(AccountError::Crypto)?;
    Ok(update_value(conn, t, key, &stored).await?)
}

/// 自社の振込先を読んで復号する(先頭の 1 行。`company_info_repo` と同じ行を選ぶ)。行が無ければ Ok(None)
pub async fn read_company_account(pool: &PgPool) -> Result<Option<AccountNumber>, AccountError> {
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM s_company_info ORDER BY id LIMIT 1")
        .fetch_optional(pool)
        .await?;
    match id {
        None => Ok(None),
        Some(id) => read_account(pool, Target::CompanyInfo, &id.to_string()).await,
    }
}

/// パートナーの請求先口座を読んで復号する(行が無ければ Ok(None))
pub async fn read_partner_account(pool: &PgPool, partner_id: &str) -> Result<Option<AccountNumber>, AccountError> {
    read_account(pool, Target::Partner, partner_id).await
}

/// 1 行の口座番号を暗号化して書く(トランザクションの中で使う。プロセスの設定を使う)
pub async fn write_account_tx(conn: &mut PgConnection, t: Target, key: &str, value: &AccountNumber) -> Result<u64, AccountError> {
    let dc = data_crypto::global().map_err(AccountError::Crypto)?;
    write_account_with(dc, conn, t, key, value).await
}

/// 1 行の口座番号を読んで復号する(プロセスの設定を使う。通常の画面・帳票はこちら)
pub async fn read_account(pool: &PgPool, t: Target, key: &str) -> Result<Option<AccountNumber>, AccountError> {
    let dc = data_crypto::global().map_err(AccountError::Crypto)?;
    let mut conn = pool.acquire().await?;
    read_account_with(dc, &mut conn, t, key).await
}

/// 1 行の口座番号を暗号化して書く(プロセスの設定を使う。通常の保存はこちら)
pub async fn write_account(pool: &PgPool, t: Target, key: &str, value: &AccountNumber) -> Result<u64, AccountError> {
    let dc = data_crypto::global().map_err(AccountError::Crypto)?;
    let mut conn = pool.acquire().await?;
    write_account_with(dc, &mut conn, t, key, value).await
}
