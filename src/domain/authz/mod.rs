//! domain/authz — ロールと組織に基づく認可(DEMO-000173)。
//! 設計: docs/design/設計書_ロールと組織に基づく閲覧範囲.md、詳細: docs/design/詳細設計書_ロールと組織に基づく閲覧範囲.md
pub mod admin_input;
pub mod decision;
pub mod list_filter;
pub mod org_tree;
pub mod owner_transfer;
pub mod permission;
pub mod redact;
pub mod scope;
pub mod stamp;
pub mod standard_roles;
pub mod view_all;

pub use permission::Permission;
pub use decision::Authz;
