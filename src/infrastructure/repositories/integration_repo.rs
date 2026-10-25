/// infrastructure/repositories/integration_repo.rs — 外部連携のつなぎ先(s_integration_target)と変更の記録(DEMO-000215)
///
/// 秘密の設定は暗号化したまま出し入れする(復号は infrastructure/integrations の SecretCipher)。
/// 画面向けの一覧(`TargetView`)には、暗号化した秘密も含めない(`has_secret` だけ)。

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

/// つなぎ先の行(組み立て用。暗号化した秘密を含む)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetRow {
    pub id: i64,
    pub use_id: String,
    pub label: String,
    pub provider: String,
    pub config: Value,
    pub secret_enc: Option<Vec<u8>>,
    pub secret_nonce: Option<Vec<u8>>,
    pub secret_key_id: Option<String>,
    pub enabled: bool,
}

/// 画面向けの行(秘密を含まない)
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TargetView {
    pub id: i64,
    pub use_id: String,
    pub label: String,
    pub provider: String,
    pub config: Value,
    pub has_secret: bool,
    pub enabled: bool,
    pub last_check_at: Option<DateTime<Utc>>,
    pub last_check_ok: Option<bool>,
    pub last_check_detail: Option<Value>,
    pub last_test_at: Option<DateTime<Utc>>,
    pub last_test_ok: Option<bool>,
    pub last_test_by_name: Option<String>,
    pub last_test_detail: Option<String>,
    pub last_delivery_at: Option<DateTime<Utc>>,
    pub last_delivery_ok: Option<bool>,
    pub last_delivery_detail: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChangeView {
    pub changed_at: DateTime<Utc>,
    pub changed_by_name: Option<String>,
    pub use_id: String,
    pub summary: String,
}

/// 秘密を暗号化したもの(書き込み用)
pub struct SealedSecret<'a> {
    pub key_id: &'a str,
    pub nonce: &'a [u8],
    pub ciphertext: &'a [u8],
}

pub async fn list_enabled_rows(pool: &PgPool) -> Result<Vec<TargetRow>> {
    Ok(sqlx::query_as::<_, TargetRow>(
        "SELECT id, use_id, label, provider, config, secret_enc, secret_nonce, secret_key_id, enabled \
           FROM s_integration_target WHERE enabled ORDER BY use_id, id",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_all_rows(pool: &PgPool) -> Result<Vec<TargetRow>> {
    Ok(sqlx::query_as::<_, TargetRow>(
        "SELECT id, use_id, label, provider, config, secret_enc, secret_nonce, secret_key_id, enabled \
           FROM s_integration_target ORDER BY use_id, id",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn find_row(pool: &PgPool, id: i64) -> Result<Option<TargetRow>> {
    Ok(sqlx::query_as::<_, TargetRow>(
        "SELECT id, use_id, label, provider, config, secret_enc, secret_nonce, secret_key_id, enabled \
           FROM s_integration_target WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_views(pool: &PgPool) -> Result<Vec<TargetView>> {
    Ok(sqlx::query_as::<_, TargetView>(
        r#"SELECT t.id, t.use_id, t.label, t.provider, t.config, (t.secret_enc IS NOT NULL) AS has_secret, t.enabled,
                  t.last_check_at, t.last_check_ok, t.last_check_detail,
                  t.last_test_at, t.last_test_ok, u.username AS last_test_by_name, t.last_test_detail,
                  t.last_delivery_at, t.last_delivery_ok, t.last_delivery_detail
             FROM s_integration_target t
             LEFT JOIN s_user u ON u.id = t.last_test_by
            ORDER BY t.use_id, t.id"#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn insert_target(
    pool: &PgPool,
    use_id: &str,
    label: &str,
    provider: &str,
    config: &Value,
    secret: Option<SealedSecret<'_>>,
    by: i64,
) -> Result<i64> {
    let (enc, nonce, kid) = match &secret {
        Some(s) => (Some(s.ciphertext), Some(s.nonce), Some(s.key_id)),
        None => (None, None, None),
    };
    Ok(sqlx::query_scalar(
        r#"INSERT INTO s_integration_target (use_id, label, provider, config, secret_enc, secret_nonce, secret_key_id, updated_by)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id"#,
    )
    .bind(use_id)
    .bind(label)
    .bind(provider)
    .bind(config)
    .bind(enc)
    .bind(nonce)
    .bind(kid)
    .bind(by)
    .fetch_one(pool)
    .await?)
}

/// 名前・設定・有効/無効を変える。秘密は `Some` のときだけ入れ替える
pub async fn update_target(
    pool: &PgPool,
    id: i64,
    label: &str,
    config: &Value,
    enabled: bool,
    secret: Option<SealedSecret<'_>>,
    by: i64,
) -> Result<()> {
    match secret {
        Some(s) => {
            sqlx::query(
                r#"UPDATE s_integration_target
                      SET label = $2, config = $3, enabled = $4, secret_enc = $5, secret_nonce = $6, secret_key_id = $7,
                          updated_by = $8, updated_at = now()
                    WHERE id = $1"#,
            )
            .bind(id)
            .bind(label)
            .bind(config)
            .bind(enabled)
            .bind(s.ciphertext)
            .bind(s.nonce)
            .bind(s.key_id)
            .bind(by)
            .execute(pool)
            .await?;
        }
        None => {
            sqlx::query(
                "UPDATE s_integration_target SET label = $2, config = $3, enabled = $4, updated_by = $5, updated_at = now() WHERE id = $1",
            )
            .bind(id)
            .bind(label)
            .bind(config)
            .bind(enabled)
            .bind(by)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// 鍵の入れ替え: 秘密だけを書き換える(更新者・更新日時は変えない)
pub async fn replace_secret(pool: &PgPool, id: i64, s: SealedSecret<'_>) -> Result<()> {
    sqlx::query("UPDATE s_integration_target SET secret_enc = $2, secret_nonce = $3, secret_key_id = $4 WHERE id = $1")
        .bind(id)
        .bind(s.ciphertext)
        .bind(s.nonce)
        .bind(s.key_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn record_check(pool: &PgPool, id: i64, ok: bool, detail: &Value) -> Result<()> {
    sqlx::query("UPDATE s_integration_target SET last_check_at = now(), last_check_ok = $2, last_check_detail = $3 WHERE id = $1")
        .bind(id)
        .bind(ok)
        .bind(detail)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn record_test(pool: &PgPool, id: i64, ok: bool, by: i64, detail: &str) -> Result<()> {
    sqlx::query(
        "UPDATE s_integration_target SET last_test_at = now(), last_test_ok = $2, last_test_by = $3, last_test_detail = $4 WHERE id = $1",
    )
    .bind(id)
    .bind(ok)
    .bind(by)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn record_delivery(pool: &PgPool, id: i64, ok: bool, detail: &str) -> Result<()> {
    sqlx::query(
        "UPDATE s_integration_target SET last_delivery_at = now(), last_delivery_ok = $2, last_delivery_detail = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(ok)
    .bind(detail)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_change(pool: &PgPool, by: i64, use_id: &str, target_id: Option<i64>, summary: &str) -> Result<()> {
    sqlx::query("INSERT INTO h_integration_change (changed_by, use_id, target_id, summary) VALUES ($1, $2, $3, $4)")
        .bind(by)
        .bind(use_id)
        .bind(target_id)
        .bind(summary)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_changes(pool: &PgPool, limit: i64) -> Result<Vec<ChangeView>> {
    Ok(sqlx::query_as::<_, ChangeView>(
        r#"SELECT c.changed_at, u.username AS changed_by_name, c.use_id, c.summary
             FROM h_integration_change c
             LEFT JOIN s_user u ON u.id = c.changed_by
            ORDER BY c.changed_at DESC
            LIMIT $1"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

// ── 毎朝の点検の記録(h_integration_daily_check。DEMO-000216) ──

/// 毎朝の点検の1回分
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DailyCheck {
    pub run_at: DateTime<Utc>,
    pub ok: bool,
    pub problems: Value,
    pub signature: String,
    pub notified: bool,
}

/// 直近の1回(前回との違いを見るため)
pub async fn last_daily_check(pool: &PgPool) -> Result<Option<DailyCheck>> {
    Ok(sqlx::query_as::<_, DailyCheck>(
        "SELECT run_at, ok, problems, signature, notified FROM h_integration_daily_check ORDER BY run_at DESC, id DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn insert_daily_check(
    pool: &PgPool,
    problems: &[String],
    signature: &str,
    notified: bool,
    notify_detail: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO h_integration_daily_check (ok, problems, signature, notified, notify_detail) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(problems.is_empty())
    .bind(serde_json::json!(problems))
    .bind(signature)
    .bind(notified)
    .bind(notify_detail)
    .execute(pool)
    .await?;
    Ok(())
}
