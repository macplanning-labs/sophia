/// infrastructure/repositories/workflow_repo.rs — ワークフローCRUD

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::workflow::*;

/// ワークフロー定義取得
pub async fn find_definition(pool: &PgPool, code: &str) -> Result<Option<WorkflowDefinition>> {
    let row = sqlx::query_as::<_, WorkflowDefinition>(
        "SELECT * FROM m_workflow_definition WHERE code = $1 AND is_active = true"
    )
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// ワークフローインスタンス取得
pub async fn find_instance(pool: &PgPool, target_type: &str, target_id: &str) -> Result<Option<WorkflowInstance>> {
    let row = sqlx::query_as::<_, WorkflowInstance>(
        "SELECT * FROM t_workflow_instance WHERE target_type = $1 AND target_id = $2 AND is_completed = false"
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 現在のステップ取得
pub async fn current_step(pool: &PgPool, target_type: &str, target_id: &str) -> Result<Option<WorkflowStep>> {
    let row = sqlx::query_as::<_, WorkflowStep>(
        r#"
        SELECT ws.* FROM m_workflow_step ws
        JOIN t_workflow_instance wi ON ws.id = wi.current_step_id
        WHERE wi.target_type = $1 AND wi.target_id = $2 AND wi.is_completed = false
        "#
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// ワークフロー定義のステップ一覧
pub async fn list_steps(pool: &PgPool, definition_id: i64) -> Result<Vec<WorkflowStep>> {
    let rows = sqlx::query_as::<_, WorkflowStep>(
        "SELECT * FROM m_workflow_step WHERE definition_id = $1 ORDER BY sort_order"
    )
    .bind(definition_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// ワークフローログ取得
pub async fn list_logs(pool: &PgPool, target_type: &str, target_id: &str) -> Result<Vec<WorkflowLog>> {
    let rows = sqlx::query_as::<_, WorkflowLog>(
        "SELECT * FROM h_workflow_log WHERE target_type = $1 AND target_id = $2 ORDER BY created_at DESC"
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// ステップID取得（ワークフロー定義コード + ステップコードから）
pub async fn find_step_id_by_definition_and_code(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    definition_code: &str,
    step_code: &str,
) -> Result<Option<i64>> {
    let step_id: Option<i64> = sqlx::query_scalar(
        r#"
        SELECT s.id
        FROM m_workflow_step s
        JOIN m_workflow_definition d ON d.id = s.definition_id
        WHERE d.code = $1 AND s.code = $2
        "#,
    )
    .bind(definition_code)
    .bind(step_code)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(step_id)
}

/// ワークフローログ記録
pub async fn insert_document_workflow_log(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    target_type: &str,
    target_id: &str,
    from_step_id: Option<i64>,
    to_step_id: Option<i64>,
    changed_by_id: i64,
    note: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO h_workflow_log
            (target_type, target_id, from_step_id, to_step_id, changed_by_id, note)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(target_type)
    .bind(target_id)
    .bind(from_step_id)
    .bind(to_step_id)
    .bind(changed_by_id)
    .bind(note)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 前方遷移取得
pub async fn find_forward_transition(pool: &PgPool, from_step_id: i64) -> Result<Option<crate::domain::models::workflow::WorkflowTransition>> {
    let row = sqlx::query_as::<_, crate::domain::models::workflow::WorkflowTransition>(
        "SELECT * FROM m_workflow_transition WHERE from_step_id = $1 AND is_back = false ORDER BY id LIMIT 1"
    )
    .bind(from_step_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 後方遷移取得
pub async fn find_back_transition(pool: &PgPool, from_step_id: i64) -> Result<Option<crate::domain::models::workflow::WorkflowTransition>> {
    let row = sqlx::query_as::<_, crate::domain::models::workflow::WorkflowTransition>(
        "SELECT * FROM m_workflow_transition WHERE from_step_id = $1 AND is_back = true ORDER BY id LIMIT 1"
    )
    .bind(from_step_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// ステップID取得
pub async fn find_step_by_id(pool: &PgPool, step_id: i64) -> Result<Option<WorkflowStep>> {
    let row = sqlx::query_as::<_, WorkflowStep>(
        "SELECT * FROM m_workflow_step WHERE id = $1"
    )
    .bind(step_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 定義の最初のステップ取得
pub async fn find_first_step(pool: &PgPool, definition_id: i64) -> Result<Option<WorkflowStep>> {
    let row = sqlx::query_as::<_, WorkflowStep>(
        "SELECT * FROM m_workflow_step WHERE definition_id = $1 ORDER BY sort_order LIMIT 1"
    )
    .bind(definition_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// ワークフローインスタンス作成
pub async fn insert_instance(
    pool: &PgPool,
    definition_id: i64,
    target_type: &str,
    target_id: &str,
    first_step_id: i64,
) -> Result<WorkflowInstance> {
    let row = sqlx::query_as::<_, WorkflowInstance>(
        "INSERT INTO t_workflow_instance (definition_id, target_type, target_id, current_step_id) VALUES ($1, $2, $3, $4) RETURNING *"
    )
    .bind(definition_id)
    .bind(target_type)
    .bind(target_id)
    .bind(first_step_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

/// インスタンスのステップ更新
pub async fn update_instance_step(
    pool: &PgPool,
    instance_id: i64,
    step_id: i64,
    is_completed: bool,
) -> Result<()> {
    if is_completed {
        sqlx::query(
            "UPDATE t_workflow_instance SET current_step_id = $1, is_completed = true, completed_at = NOW() WHERE id = $2"
        )
        .bind(step_id)
        .bind(instance_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE t_workflow_instance SET current_step_id = $1 WHERE id = $2"
        )
        .bind(step_id)
        .bind(instance_id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// ワークフロー遷移ログ記録
pub async fn insert_workflow_log(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    from_step_id: i64,
    to_step_id: i64,
    changed_by_id: Option<i64>,
    note: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO h_workflow_log (target_type, target_id, from_step_id, to_step_id, changed_by_id, note) VALUES ($1, $2, $3, $4, $5, $6)"
    )
    .bind(target_type)
    .bind(target_id)
    .bind(from_step_id)
    .bind(to_step_id)
    .bind(changed_by_id)
    .bind(note)
    .execute(pool)
    .await?;
    Ok(())
}

/// 定義・ステップコードからステップを取得（move_to 用）
pub async fn find_step_by_definition_and_code(pool: &PgPool, definition_id: i64, code: &str) -> Result<Option<WorkflowStep>> {
    let row = sqlx::query_as::<_, WorkflowStep>(
        "SELECT * FROM m_workflow_step WHERE definition_id = $1 AND code = $2"
    )
    .bind(definition_id)
    .bind(code)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
