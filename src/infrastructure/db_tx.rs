//! DBアクセスの共通部品: エラーのログ出力と、トランザクションの安全な確定
//!
//! 背景（2026-10-01 の障害）: 取引先への請求書の一括発行で、トランザクションの途中の文が失敗したのに、
//! 失敗を捨てて先へ進み、最後に確定していた。PostgreSQL は、失敗状態のトランザクションに COMMIT を
//! 送ると、エラーではなく「ROLLBACK」として扱うため、アプリは確定できたと誤解し、成功と表示しつつ
//! 何も保存されなかった。同じ事象を、ここで防ぐ。

use std::fmt::Debug;

/// 結果がエラーなら、呼び出し位置つきでログに出して、そのまま返す。
///
/// `.ok()` や `.unwrap_or_default()` で、エラーを「なし」「空」として扱う箇所でも、
/// 失敗の事実がログに残るようにする（動作は変えない）。
///
/// ```ignore
/// let row = repo::find(&pool, id).await.log_err().ok().flatten();
/// ```
pub trait LogErr<T, E> {
    fn log_err(self) -> Result<T, E>;
}

impl<T, E: Debug> LogErr<T, E> for Result<T, E> {
    #[track_caller]
    fn log_err(self) -> Result<T, E> {
        if let Err(e) = &self {
            let loc = std::panic::Location::caller();
            tracing::error!("[DBエラー] {}:{} {:?}", loc.file(), loc.line(), e);
        }
        self
    }
}

/// トランザクションを確定する。途中で失敗した文があれば、確定せずに取り消して、エラーを返す。
///
/// 失敗状態のトランザクションに COMMIT を送っても、sqlx はエラーを返さない（PostgreSQL が
/// 「ROLLBACK」として扱うため）。そこで、確定の前に1文流して、失敗状態を検出する。
/// 失敗状態なら、この文が「current transaction is aborted」で失敗する。
pub async fn commit_checked(mut tx: sqlx::Transaction<'_, sqlx::Postgres>) -> Result<(), sqlx::Error> {
    if let Err(e) = sqlx::query("SELECT 1").execute(&mut *tx).await {
        tracing::error!("[DBエラー] トランザクションが失敗状態のため、確定せずに取り消します: {:?}", e);
        return Err(e); // tx は drop されて ROLLBACK される
    }
    tx.commit().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_err_returns_the_result_unchanged() {
        let ok: Result<i32, String> = Ok(1);
        assert_eq!(ok.log_err(), Ok(1));
        let err: Result<i32, String> = Err("boom".to_string());
        assert_eq!(err.log_err(), Err("boom".to_string()));
    }

    /// DBを使う検証（普段は動かさない。`cargo test db_tx -- --ignored` で、DATABASE_URL のDBに対して動かす）。
    ///
    /// 失敗した文のあとの確定を、`commit_checked` が Err にして、データが残らないこと。
    /// （素の `tx.commit()` は、同じ状況で Ok を返す。これが 2026-10-01 の障害の原因だった）
    #[tokio::test]
    #[ignore = "DB（DATABASE_URL）が必要"]
    async fn commit_checked_rejects_a_transaction_that_has_already_failed() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://sophia:sophia@localhost:5432/sophia".to_string());
        let pool = sqlx::PgPool::connect(&url).await.expect("DB に接続できません");

        // 1. 素の commit() は、失敗状態のトランザクションでも Ok を返す（取り消されたのに成功に見える）
        let mut tx = pool.begin().await.expect("begin");
        sqlx::query("CREATE TEMP TABLE _commit_check (id int)").execute(&mut *tx).await.expect("temp table");
        let failed = sqlx::query("INSERT INTO _commit_check (no_such_column) VALUES (1)").execute(&mut *tx).await;
        assert!(failed.is_err(), "存在しない列への書き込みは失敗するはず");
        let plain = tx.commit().await;
        assert!(plain.is_ok(), "素の commit() は、失敗状態でも Ok を返す（これが障害の原因）: {plain:?}");

        // 2. commit_checked は、同じ状況で Err を返す
        let mut tx = pool.begin().await.expect("begin");
        sqlx::query("CREATE TEMP TABLE _commit_check2 (id int)").execute(&mut *tx).await.expect("temp table");
        let failed = sqlx::query("INSERT INTO _commit_check2 (no_such_column) VALUES (1)").execute(&mut *tx).await;
        assert!(failed.is_err());
        let checked = commit_checked(tx).await;
        assert!(checked.is_err(), "commit_checked は、失敗状態のトランザクションを Err にするはず");

        // 3. 正常なトランザクションは、commit_checked で確定できる
        let mut tx = pool.begin().await.expect("begin");
        sqlx::query("CREATE TEMP TABLE _commit_check3 (id int)").execute(&mut *tx).await.expect("temp table");
        sqlx::query("INSERT INTO _commit_check3 (id) VALUES (1)").execute(&mut *tx).await.expect("insert");
        assert!(commit_checked(tx).await.is_ok());
    }
}
