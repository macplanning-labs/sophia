/// domain/services/document_workflow.rs — 汎用ドキュメント・ステータスワークフロー
///
/// 経費申請など「ステータス文字列 + アクション」で遷移する業務ドキュメント向け。
/// 既存の DB 駆動エンジン（`workflow_engine` / 発注・受注サイクル）とは別に、
/// コード上で遷移可否・権限・監査ログ（`h_workflow_log`）を一括管理する。
///
/// 他機能への再利用時は `DocumentKind` と遷移表を追加し、
/// エンティティ更新は呼び出し側の `StatusApplier` に委譲する。

use anyhow::{bail, Result};

/// ワークフロー対象ドキュメント種別（`m_workflow_definition.code` と対応）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    ExpenseRequest,
}

impl DocumentKind {
    pub fn definition_code(self) -> &'static str {
        match self {
            Self::ExpenseRequest => "EXPENSE_REQUEST",
        }
    }

    pub fn target_type(self) -> &'static str {
        match self {
            Self::ExpenseRequest => "expense_request",
        }
    }
}

/// 実行アクション
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowAction {
    /// DRAFT → PENDING
    Submit,
    /// PENDING → APPROVED
    Approve,
    /// PENDING → REJECTED
    Reject,
    /// REJECTED → PENDING
    Resubmit,
    /// APPROVED → PENDING（未精算時のみ）
    Unapprove,
    /// APPROVED → PAID
    MarkPaid,
}

impl WorkflowAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Resubmit => "resubmit",
            Self::Unapprove => "unapprove",
            Self::MarkPaid => "mark_paid",
        }
    }

    pub fn note(self) -> &'static str {
        match self {
            Self::Submit => "申請",
            Self::Approve => "承認",
            Self::Reject => "差戻し/却下",
            Self::Resubmit => "再申請",
            Self::Unapprove => "承認取り消し",
            Self::MarkPaid => "精算済",
        }
    }
}

/// 実行者コンテキスト
#[derive(Debug, Clone)]
pub struct ActorContext {
    pub user_id: i64,
    /// 承認・差戻し・承認取り消しなど管理操作ができるか
    pub can_manage: bool,
    /// ドキュメント所有者か（申請者本人）
    pub is_owner: bool,
}

/// 遷移評価結果
#[derive(Debug, Clone)]
pub struct TransitionPlan {
    pub from_status: String,
    pub to_status: String,
    pub action: WorkflowAction,
}

/// 内容編集（明細・領収書含む）が可能なステータスか
pub fn can_edit_content(status: &str) -> bool {
    matches!(status, "PENDING" | "DRAFT")
}

/// 遷移可否を評価し、遷移先ステータスを返す（副作用なし）
pub fn evaluate_transition(
    kind: DocumentKind,
    from_status: &str,
    action: WorkflowAction,
    actor: &ActorContext,
) -> Result<TransitionPlan> {
    let _ = kind; // 将来ドキュメント種別ごとの差分に使用

    let to_status = match (from_status, action) {
        ("DRAFT", WorkflowAction::Submit) => "PENDING",
        ("PENDING", WorkflowAction::Approve) => "APPROVED",
        ("PENDING", WorkflowAction::Reject) => "REJECTED",
        ("REJECTED", WorkflowAction::Resubmit) => "PENDING",
        ("APPROVED", WorkflowAction::Unapprove) => "PENDING",
        ("APPROVED", WorkflowAction::MarkPaid) => "PAID",
        ("PAID", WorkflowAction::Unapprove) => {
            bail!("精算済の申請は申請中に戻せません（給与/振込確定との整合性のため）");
        }
        (status, action) => {
            bail!(
                "ステータス「{}」からアクション「{}」への遷移はできません",
                status,
                action.as_str()
            );
        }
    };

    match action {
        WorkflowAction::Approve | WorkflowAction::Reject | WorkflowAction::Unapprove | WorkflowAction::MarkPaid => {
            if !actor.can_manage {
                bail!("この操作を行う権限がありません");
            }
        }
        WorkflowAction::Submit | WorkflowAction::Resubmit => {
            if !actor.is_owner && !actor.can_manage {
                bail!("申請者本人のみ実行できます");
            }
        }
    }

    Ok(TransitionPlan {
        from_status: from_status.to_string(),
        to_status: to_status.to_string(),
        action,
    })
}

/// 遷移ログをトランザクション内に記録する（エンティティ更新の後に呼ぶ）
pub async fn write_transition_log(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: DocumentKind,
    document_id: &str,
    plan: &TransitionPlan,
    actor: &ActorContext,
) -> Result<()> {
    use crate::infrastructure::repositories::workflow_repo;

    let from_step_id = workflow_repo::find_step_id_by_definition_and_code(tx, kind.definition_code(), &plan.from_status).await?;
    let to_step_id = workflow_repo::find_step_id_by_definition_and_code(tx, kind.definition_code(), &plan.to_status).await?;

    workflow_repo::insert_document_workflow_log(
        tx,
        kind.target_type(),
        document_id,
        from_step_id,
        to_step_id,
        actor.user_id,
        plan.action.note(),
    ).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> ActorContext {
        ActorContext { user_id: 1, can_manage: true, is_owner: false }
    }

    fn owner() -> ActorContext {
        ActorContext { user_id: 2, can_manage: false, is_owner: true }
    }

    #[test]
    fn approve_pending_ok() {
        let plan = evaluate_transition(
            DocumentKind::ExpenseRequest,
            "PENDING",
            WorkflowAction::Approve,
            &manager(),
        )
        .unwrap();
        assert_eq!(plan.to_status, "APPROVED");
    }

    #[test]
    fn unapprove_approved_ok() {
        let plan = evaluate_transition(
            DocumentKind::ExpenseRequest,
            "APPROVED",
            WorkflowAction::Unapprove,
            &manager(),
        )
        .unwrap();
        assert_eq!(plan.to_status, "PENDING");
    }

    #[test]
    fn unapprove_paid_blocked() {
        let err = evaluate_transition(
            DocumentKind::ExpenseRequest,
            "PAID",
            WorkflowAction::Unapprove,
            &manager(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("精算済"));
    }

    #[test]
    fn owner_cannot_approve() {
        let err = evaluate_transition(
            DocumentKind::ExpenseRequest,
            "PENDING",
            WorkflowAction::Approve,
            &owner(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("権限"));
    }

    #[test]
    fn resubmit_rejected_ok() {
        let plan = evaluate_transition(
            DocumentKind::ExpenseRequest,
            "REJECTED",
            WorkflowAction::Resubmit,
            &owner(),
        )
        .unwrap();
        assert_eq!(plan.to_status, "PENDING");
    }

    #[test]
    fn can_edit_only_pending_or_draft() {
        assert!(can_edit_content("PENDING"));
        assert!(can_edit_content("DRAFT"));
        assert!(!can_edit_content("APPROVED"));
        assert!(!can_edit_content("PAID"));
        assert!(!can_edit_content("REJECTED"));
    }
}
