/// infrastructure/repositories/test_support.rs — Repository層のDB統合テスト用ヘルパー
///
/// 接続先は、`scripts/cargo-test-hermetic.sh` が起動する使い捨てDB(Docker)を、
/// 環境変数 TEST_DATABASE_URL / DATABASE_URL で受け取る。手元のPostgreSQL(localhost:5432)には
/// 既定でもつながない(未設定なら、原因を示して失敗する)。BEGIN〜ROLLBACKで副作用を消す方式を採る。
///
/// `PgPoolOptions::max_connections(1)`にすることで、プールが返す接続が常に同一の
/// 物理コネクションになる。これにより`BEGIN`〜`ROLLBACK`が複数回の`pool.execute()`呼び出しを
/// またいで1つのトランザクションとして機能する（sqlxはプール全体に対する
/// トランザクションAPIを提供しないための回避策）。テストがpanicした場合もROLLBACKに
/// 到達しないだけで、コネクションのdrop時に未コミットのトランザクションは自動的に
/// 破棄されるため、実データへの影響は残らない。
///
/// 使い捨てDBでも、以下の理由でテスト同士が干渉しない:
/// - 全操作がコミットされない（テスト終了時に明示的ROLLBACK、panic時はconnection dropで暗黙ROLLBACK）
/// - 他のテストは各々が新しいプール（＝新しい物理コネクション）を張るため、
///   並列実行してもトランザクションが混ざらない

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("TEST_DATABASE_URL が未設定です。DBを使うテストは ./scripts/cargo-test-hermetic.sh で実行してください(手元のPostgreSQLには既定でつながりません)")
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
        .expect("テスト用DB接続に失敗しました。使い捨てDBが起動しているか(./scripts/cargo-test-hermetic.sh 経由か)確認してください");

    sqlx::query("BEGIN").execute(&pool).await.expect("BEGIN failed");

    f(pool.clone()).await;

    let _ = sqlx::query("ROLLBACK").execute(&pool).await;
}

/// テスト用の職員ユーザー(有効・職員)を、同じトランザクションの中に用意する(ROLLBACK で消える)。
/// 使い捨てDBは空なので、「ユーザーID 1 が居る」ような手元のDBの中身に頼らない。
pub async fn ensure_staff_user(pool: &PgPool, id: i64) {
    sqlx::query(
        "INSERT INTO s_user (id, email, password, username, is_active, is_staff) \
         VALUES ($1, $2, 'x', $3, TRUE, TRUE) \
         ON CONFLICT (id) DO UPDATE SET is_active = TRUE, is_staff = TRUE",
    )
    .bind(id)
    .bind(format!("test-user-{id}@example.invalid"))
    .bind(format!("test-user-{id}"))
    .execute(pool)
    .await
    .expect("テスト用ユーザーの作成に失敗しました");
}
