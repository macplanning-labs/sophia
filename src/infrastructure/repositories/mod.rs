// infrastructure/repositories — SQLクエリ実装モジュール
//
// 各エンティティのCRUD操作をここに集約する。
// 共通パターン（list_all, find_by_id, count）は base.rs のマクロで生成。

#[macro_use]
pub mod base;

pub mod client_repo;
pub mod partner_repo;
pub mod project_repo;
pub mod order_repo;
pub mod billing_repo;
pub mod timesheet_repo;
pub mod task_repo;
pub mod payroll_repo;
pub mod paid_leave_repo;
pub mod workflow_repo;
pub mod mail_repo;
pub mod received_email_repo;
pub mod mail_brief_repo;
pub mod employee_repo;
pub mod master_repo;
pub mod invite_repo;
pub mod expense_repo;
pub mod mobile_upload_repo;
pub mod security_repo;
pub mod user_repo;
pub mod auth_repo;
pub mod attempt_lock_store;
pub mod jwt_blacklist_repo;
pub mod company_info_repo;
pub mod peppol_repo;
pub mod tax_rate_repo;
pub mod settlement_repo;
pub mod reminder_repo;
pub mod email_template_repo;
pub mod api_key_repo;
pub mod api_access_log_repo;

#[cfg(test)]
pub(crate) mod test_support;
