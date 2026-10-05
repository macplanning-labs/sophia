/// infrastructure/repositories/mobile_upload_repo.rs — スマホ連携アップロード用トークン(s_mobile_upload_token) CRUD
///
/// PCの経費申請画面から発行したワンタイムトークンをスマホがQRコード経由で受け取り、
/// 領収書画像をアップロードするためのトークン管理。
/// トークンの有効期限は10分・使い切り（`used_at`が立ったら再利用不可）。

use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::models::expense::MobileUploadToken;

/// トークンの有効期限（分）
pub const TOKEN_TTL_MINUTES: i64 = 10;

/// アップロード可能な最大ファイルサイズ（バイト）
pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// トークンを発行する（明細1件に対して1トークン）
pub async fn issue_token(pool: &PgPool, expense_request_item_id: i64) -> Result<MobileUploadToken> {
    let expires_at = Utc::now() + Duration::minutes(TOKEN_TTL_MINUTES);

    let token = sqlx::query_as::<_, MobileUploadToken>(
        r#"
        INSERT INTO s_mobile_upload_token (expense_request_item_id, expires_at)
        VALUES ($1, $2)
        RETURNING token, expense_request_item_id, created_at, expires_at, used_at
        "#
    )
    .bind(expense_request_item_id)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;

    Ok(token)
}

/// トークンを取得する（有効性の判定は呼び出し側で`MobileUploadToken::is_valid`を使う）
pub async fn find_token(pool: &PgPool, token: Uuid) -> Result<Option<MobileUploadToken>> {
    let row = sqlx::query_as::<_, MobileUploadToken>(
        "SELECT token, expense_request_item_id, created_at, expires_at, used_at FROM s_mobile_upload_token WHERE token = $1"
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// トークンを使用済みにする（更新件数を返す。既に使用済み/存在しない場合は0）
pub async fn mark_used(pool: &PgPool, token: Uuid) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE s_mobile_upload_token SET used_at = NOW() WHERE token = $1 AND used_at IS NULL"
    )
    .bind(token)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}
