/// domain/models/workflow.rs — ワークフローエンティティ
///
/// DB駆動ワークフロー。Django の GenericForeignKey を
/// Rust の型安全 enum（WorkflowTarget）に置換。

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

/// ワークフロー対象の型安全な表現
/// Django の ContentType + GenericFK を置換
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum WorkflowTarget {
    PurchaseOrder,
    ReceivedOrder,
}

impl WorkflowTarget {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PurchaseOrder => "purchase_order",
            Self::ReceivedOrder => "received_order",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "purchase_order" => Some(Self::PurchaseOrder),
            "received_order" => Some(Self::ReceivedOrder),
            _ => None,
        }
    }
}

/// ワークフロー定義
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowDefinition {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub description: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// ワークフローステップ
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowStep {
    pub id: i64,
    pub definition_id: i64,
    pub code: String,
    pub name: String,
    pub sort_order: i32,
    pub is_terminal: bool,
    pub icon: String,
    pub allowed_mail_types: serde_json::Value,
}

/// ワークフロー遷移ルール
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowTransition {
    pub id: i64,
    pub from_step_id: i64,
    pub to_step_id: i64,
    pub is_back: bool,
    pub description: String,
}

/// ワークフローインスタンス（実行中のワークフロー）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowInstance {
    pub id: i64,
    pub definition_id: i64,
    pub target_type: String,
    pub target_id: String,
    pub current_step_id: i64,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub is_completed: bool,
}

/// ワークフロー遷移ログ（監査証跡）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct WorkflowLog {
    pub id: i64,
    pub target_type: String,
    pub target_id: String,
    pub from_step_id: Option<i64>,
    pub to_step_id: Option<i64>,
    pub changed_by_id: Option<i64>,
    pub note: String,
    pub created_at: DateTime<Utc>,
}
