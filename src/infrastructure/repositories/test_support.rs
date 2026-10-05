/// infrastructure/repositories/test_support.rs — Repository層のDB統合テスト用ヘルパー
///
/// ローカルDBユーザーに`CREATEDB`権限が無い環境でも動くよう、テスト専用のephemeral DBは
/// 作らず、既存のローカル開発DB（`postgresql://sophia:${POSTGRES_PASSWORD}@localhost:5432/sophia`）に対して
/// BEGIN〜ROLLBACKで副作用を消す方式を採る。
///
/// `PgPoolOptions::max_connections(1)`にすることで、プールが返す接続が常に同一の
/// 物理コネクションになる。これにより`BEGIN`〜`ROLLBACK`が複数回の`pool.execute()`呼び出しを
/// またいで1つのトランザクションとして機能する（sqlxはプール全体に対する
/// トランザクションAPIを提供しないための回避策）。テストがpanicした場合もROLLBACKに
/// 到達しないだけで、コネクションのdrop時に未コミットのトランザクションは自動的に
/// 破棄されるため、実データへの影響は残らない。
///
/// ローカル開発DBを直接使うが、以下の理由で安全:
/// - 全操作がコミットされない（テスト終了時に明示的ROLLBACK、panic時はconnection dropで暗黙ROLLBACK）
/// - 他のテストは各々が新しいプール（＝新しい物理コネクション）を張るため、
///   並列実行してもトランザクションが混ざらない

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| "postgresql://sophia:${POSTGRES_PASSWORD}@localhost:5432/sophia".to_string())
}

/// クロージャ内でDB操作を行い、終了後に必ずROLLBACKする（コミットは一切行わない）。
pub async fn with_rollback<F, Fut>(f: F)
where
    F: FnOnce(PgPool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&test_database_url())
        .await
        .expect("テスト用DB接続に失敗しました。ローカルDB(postgresql://sophia:${POSTGRES_PASSWORD}@localhost:5432/sophia)が起動しているか確認してください");

    sqlx::query("BEGIN").execute(&pool).await.expect("BEGIN failed");

    f(pool.clone()).await;

    let _ = sqlx::query("ROLLBACK").execute(&pool).await;
}
