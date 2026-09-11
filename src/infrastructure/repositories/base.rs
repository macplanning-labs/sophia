/// infrastructure/repositories/base.rs — リポジトリ共通マクロ
///
/// 単純なCRUD操作（list_all, find_by_id, count）を
/// マクロで自動生成する。業務固有のクエリは各リポジトリに残す。
///
/// concat! はコンパイル時に文字列を結合するため、
/// format! と異なり実行時のSQLインジェクションリスクがない。

/// SELECT * FROM {table} ORDER BY {order} — 全件取得
macro_rules! impl_list_all {
    ($fn_name:ident, $type:ty, $table:expr, $order:expr) => {
        pub async fn $fn_name(pool: &::sqlx::PgPool) -> ::anyhow::Result<Vec<$type>> {
            let rows = ::sqlx::query_as::<_, $type>(
                concat!("SELECT * FROM ", $table, " ORDER BY ", $order),
            )
            .fetch_all(pool)
            .await?;
            Ok(rows)
        }
    };
}

/// SELECT * FROM {table} WHERE {pk} = $1 — i64 PKで1件取得
macro_rules! impl_find_by_id {
    ($fn_name:ident, $type:ty, $table:expr, $pk:expr) => {
        pub async fn $fn_name(pool: &::sqlx::PgPool, id: i64) -> ::anyhow::Result<Option<$type>> {
            let row = ::sqlx::query_as::<_, $type>(
                concat!("SELECT * FROM ", $table, " WHERE ", $pk, " = $1"),
            )
            .bind(id)
            .fetch_optional(pool)
            .await?;
            Ok(row)
        }
    };
}

/// SELECT * FROM {table} WHERE {pk} = $1 — 文字列PKで1件取得
macro_rules! impl_find_by_str_id {
    ($fn_name:ident, $type:ty, $table:expr, $pk:expr) => {
        pub async fn $fn_name(pool: &::sqlx::PgPool, id: &str) -> ::anyhow::Result<Option<$type>> {
            let row = ::sqlx::query_as::<_, $type>(
                concat!("SELECT * FROM ", $table, " WHERE ", $pk, " = $1"),
            )
            .bind(id)
            .fetch_optional(pool)
            .await?;
            Ok(row)
        }
    };
}

/// SELECT COUNT(*) FROM {table} — 件数取得
macro_rules! impl_count {
    ($fn_name:ident, $table:expr) => {
        pub async fn $fn_name(pool: &::sqlx::PgPool) -> ::anyhow::Result<i64> {
            let (count,): (i64,) =
                ::sqlx::query_as(concat!("SELECT COUNT(*) FROM ", $table))
                    .fetch_one(pool)
                    .await?;
            Ok(count)
        }
    };
}

/// SELECT * FROM {table} WHERE {fk} = $1 ORDER BY {order} — FK指定で子レコード取得
macro_rules! impl_list_by_fk {
    ($fn_name:ident, $type:ty, $table:expr, $fk:expr, $order:expr) => {
        pub async fn $fn_name(pool: &::sqlx::PgPool, fk_id: i64) -> ::anyhow::Result<Vec<$type>> {
            let rows = ::sqlx::query_as::<_, $type>(
                concat!("SELECT * FROM ", $table, " WHERE ", $fk, " = $1 ORDER BY ", $order),
            )
            .bind(fk_id)
            .fetch_all(pool)
            .await?;
            Ok(rows)
        }
    };
}

// マクロを同一クレート内で使えるようにする
