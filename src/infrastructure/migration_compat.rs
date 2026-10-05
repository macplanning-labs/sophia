/// infrastructure/migration_compat.rs — 書き換えたマイグレーションの、旧版のチェックサムを受け入れる
///
/// sqlx は、適用済みのマイグレーションの中身をチェックサム（SQL 本文の SHA-384）で確かめ、違うと起動しない。
/// 公開版（OSS）では、公開済みのマイグレーションから取引先の情報を消すために、中身を書き換えることがある
/// （DEMO-000095）。そのままでは旧版を適用済みの DB が起動できなくなるため、起動時に、ここに列挙した
/// 「(番号, 旧版のチェックサム)」と完全に一致する行だけ、チェックサムを今のファイルの値に読み替える。
/// それ以外の不一致は、従来どおり起動時に止まる（意図しない改変は見逃さない）。
///
/// 社内版では空。公開版の作成手順（OSSP の profile_transform）が、`REWRITTEN` を書き換える。
use sqlx::migrate::Migrator;
use sqlx::PgPool;

/// 書き換えたマイグレーションの (番号, 旧版のチェックサムの16進)
pub const REWRITTEN: &[(i64, &str)] = &[
    (10, "51627470fba14d15e73064546371c3bea39615597a2801c705d9996d525379f764676dfecaa638f0997b791ac4af7442"),
    (37, "f10e89bbc24457c66a215f8e962421a1566969e6c86f3739e180afee2b13f768ca1c650d68b63a7aa2f7cae8710e11b9"),
    (38, "840dc797c94108bd1e4b1112de81eb37a6b1dff2e434bd063b5b7a8f8857c51f360619ae34b9a7262bf4da45b54402c5"),
];

/// `REWRITTEN` に列挙した旧版のチェックサムを、今のファイルの値に読み替える。読み替えた行数を返す。
pub async fn accept_rewritten(pool: &PgPool, migrator: &Migrator) -> Result<u64, sqlx::Error> {
    accept_rewritten_with(pool, migrator, REWRITTEN).await
}

pub async fn accept_rewritten_with(
    pool: &PgPool,
    migrator: &Migrator,
    rewritten: &[(i64, &str)],
) -> Result<u64, sqlx::Error> {
    if rewritten.is_empty() {
        return Ok(0);
    }
    // 新規の DB（まだ一度も適用していない）には、読み替える行が無い
    let has_table: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(pool)
        .await?;
    if !has_table {
        return Ok(0);
    }
    let mut updated = 0;
    for (version, old_hex) in rewritten {
        let Some(m) = migrator.iter().find(|m| m.version == *version) else {
            tracing::warn!("[migration] 読み替え対象の番号 {version} のマイグレーションがありません");
            continue;
        };
        let Ok(old) = hex::decode(old_hex) else {
            tracing::warn!("[migration] 番号 {version} の旧版のチェックサムが16進ではありません");
            continue;
        };
        let r = sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = $2 AND checksum = $3")
            .bind(m.checksum.as_ref())
            .bind(version)
            .bind(&old)
            .execute(pool)
            .await?;
        if r.rows_affected() > 0 {
            tracing::info!("[migration] 番号 {version}: 書き換え前の版を適用済みのため、チェックサムを今の版に読み替えました");
        }
        updated += r.rows_affected();
    }
    Ok(updated)
}
