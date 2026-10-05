/// domain/services/workflow_engine.rs — ワークフロー遷移エンジン
///
/// DB駆動ワークフロー。ステップの進行・差戻し・メール送信制御・遷移ログ記録。

use anyhow::{Result, bail};
use sqlx::PgPool;

use crate::domain::models::workflow::*;
use crate::infrastructure::repositories::workflow_repo;

/// ワークフローを次のステップに進める
pub async fn advance(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    changed_by_id: Option<i64>,
    note: &str,
) -> Result<WorkflowStep> {
    let instance = workflow_repo::find_instance(pool, target_type, target_id)
        .await?;

    let instance = match instance {
        Some(i) => i,
        None => bail!("Active workflow instance not found for {}/{}", target_type, target_id),
    };

    let transition = workflow_repo::find_forward_transition(pool, instance.current_step_id)
        .await?;

    let transition = match transition {
        Some(t) => t,
        None => bail!("No forward transition from current step"),
    };

    let next_step = workflow_repo::find_step_by_id(pool, transition.to_step_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Next step not found"))?;

    workflow_repo::update_instance_step(pool, instance.id, next_step.id, next_step.is_terminal).await?;
    workflow_repo::insert_workflow_log(pool, target_type, target_id, instance.current_step_id, next_step.id, changed_by_id, note).await?;

    Ok(next_step)
}

/// ワークフローを新規開始する
pub async fn start_workflow(
    pool: &PgPool,
    definition_code: &str,
    target_type: &str,
    target_id: &str,
) -> Result<WorkflowInstance> {
    let definition = workflow_repo::find_definition(pool, definition_code)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Workflow definition not found: {}", definition_code))?;

    let first_step = workflow_repo::find_first_step(pool, definition.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("First step not found in workflow"))?;

    let instance = workflow_repo::insert_instance(pool, definition.id, target_type, target_id, first_step.id).await?;

    Ok(instance)
}

/// ワークフローを前のステップに差し戻す
pub async fn go_back(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    changed_by_id: Option<i64>,
    note: &str,
) -> Result<WorkflowStep> {
    let instance = workflow_repo::find_instance(pool, target_type, target_id)
        .await?;

    let instance = match instance {
        Some(i) => i,
        None => bail!("Active workflow instance not found for {}/{}", target_type, target_id),
    };

    let transition = workflow_repo::find_back_transition(pool, instance.current_step_id)
        .await?;

    let transition = match transition {
        Some(t) => t,
        None => bail!("No back transition from current step"),
    };

    let prev_step = workflow_repo::find_step_by_id(pool, transition.to_step_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Previous step not found"))?;

    workflow_repo::update_instance_step(pool, instance.id, prev_step.id, false).await?;
    workflow_repo::insert_workflow_log(pool, target_type, target_id, instance.current_step_id, prev_step.id, changed_by_id, note).await?;

    Ok(prev_step)
}

/// ワークフローを任意のステップに移動する
pub async fn move_to(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    target_step_code: &str,
    changed_by_id: Option<i64>,
    note: &str,
) -> Result<WorkflowStep> {
    let instance = workflow_repo::find_instance(pool, target_type, target_id)
        .await?;

    let instance = match instance {
        Some(i) => i,
        None => bail!("Active workflow instance not found for {}/{}", target_type, target_id),
    };

    let target_step = workflow_repo::find_step_by_definition_and_code(pool, instance.definition_id, target_step_code)
        .await?;

    let target_step = match target_step {
        Some(s) => s,
        None => bail!("Step '{}' not found in workflow", target_step_code),
    };

    workflow_repo::update_instance_step(pool, instance.id, target_step.id, target_step.is_terminal).await?;
    workflow_repo::insert_workflow_log(pool, target_type, target_id, instance.current_step_id, target_step.id, changed_by_id, note).await?;

    Ok(target_step)
}

/// 現在のステップでメール送信が許可されているかチェックする
pub async fn can_send_mail(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    mail_type: &str,
) -> Result<bool> {
    let instance = workflow_repo::find_instance(pool, target_type, target_id)
        .await?;

    let instance = match instance {
        Some(i) => i,
        None => return Ok(false),
    };

    let step = workflow_repo::find_step_by_id(pool, instance.current_step_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Current step not found"))?;

    let allowed = match &step.allowed_mail_types {
        serde_json::Value::Array(arr) => {
            arr.iter().any(|v| v.as_str() == Some(mail_type))
        }
        serde_json::Value::String(s) => {
            s.split(',').any(|t| t.trim() == mail_type)
        }
        _ => false,
    };
    Ok(allowed)
}
