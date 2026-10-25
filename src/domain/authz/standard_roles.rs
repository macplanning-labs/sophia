//! domain/authz/standard_roles.rs — 標準ロールと、その初期の権限セット(設計書 §5.3)。
//!
//! マイグレーションの初期データ(PR 3)と、このファイルの内容が一致することをテストで確かめる。
//! 会社が後から変えた権限セットは DB が正で、このファイルは初期値だけを表す。

use super::permission::Permission;
use super::permission::Permission::*;
use super::scope::DataScope;
use super::scope::DataScope::*;

/// 標準ロール 1 件。`grants` は (権限, スコープ) の組の初期値。
pub struct StandardRole {
    /// ロールコード(DB の `m_role.code` と一致する値)。
    pub code: &'static str,
    /// 表示名。
    pub name: &'static str,
    /// 初期の権限セット。管理者だけは空で、`admin_grants()` が全権限を返す。
    pub grants: &'static [(Permission, DataScope)],
}

/// 管理者の権限セット: 全権限を全社(GLOBAL)で持つ。
pub fn admin_grants() -> Vec<(Permission, DataScope)> {
    Permission::ALL.iter().map(|p| (*p, Global)).collect()
}

/// 標準ロールの一覧(7 件)。
pub const STANDARD_ROLES: &[StandardRole] = &[
    // 管理者。権限は `admin_grants()` で全件展開するため、ここは空。
    StandardRole { code: "admin", name: "管理者", grants: &[] },
    StandardRole {
        code: "sales_lead",
        name: "営業責任者(部門・課長)",
        grants: &[
            (ClientView, Hierarchical),
            (ClientEdit, Hierarchical),
            (PartnerView, Global),
            (PartnerEdit, Global),
            (EngineerView, Global),
            (EngineerEdit, Global),
            (ProjectView, Hierarchical),
            (ProjectEdit, Hierarchical),
            (ContractView, Hierarchical),
            (ContractEdit, Hierarchical),
            (ContractPriceView, Hierarchical),
            (ContractPriceEdit, Hierarchical),
            (OrderPurchaseView, Hierarchical),
            (OrderPurchaseEdit, Hierarchical),
            (OrderPurchaseIssue, Hierarchical),
            (OrderReceivedView, Hierarchical),
            (OrderReceivedEdit, Hierarchical),
            (BillingInvoiceView, Hierarchical),
            (TimesheetView, Hierarchical),
            (TimesheetEdit, Hierarchical),
            (TimesheetApprove, Hierarchical),
            (MailReceivedView, Global),
            (MailReceivedEdit, Global),
            (ScopeUnassigned, Global),
        ],
    },
    StandardRole {
        code: "sales",
        name: "営業担当者",
        grants: &[
            (ClientView, Private),
            (ClientEdit, Private),
            (PartnerView, Global),
            (PartnerEdit, Global),
            (EngineerView, Global),
            (EngineerEdit, Global),
            (ProjectView, Private),
            (ProjectEdit, Private),
            (ContractView, Private),
            (ContractEdit, Private),
            (ContractPriceView, Private),
            (ContractPriceEdit, Private),
            (OrderPurchaseView, Private),
            (OrderPurchaseEdit, Private),
            (OrderPurchaseIssue, Private),
            (OrderReceivedView, Private),
            (OrderReceivedEdit, Private),
            (BillingInvoiceView, Private),
            (TimesheetView, Private),
            (TimesheetEdit, Private),
            (TimesheetApprove, Private),
            (MailReceivedView, Global),
            (MailReceivedEdit, Global),
            (ScopeUnassigned, Global),
        ],
    },
    StandardRole {
        code: "accounting",
        name: "経理",
        grants: &[
            (ClientView, Global),
            (ClientEdit, Global),
            (PartnerView, Global),
            (PartnerEdit, Global),
            (PartnerBankView, Global),
            (PartnerBankEdit, Global),
            (EngineerView, Global),
            (ProjectView, Global),
            (ContractView, Global),
            (ContractPriceView, Global),
            (OrderPurchaseView, Global),
            (OrderReceivedView, Global),
            (BillingInvoiceView, Global),
            (BillingInvoiceEdit, Global),
            (BillingInvoiceIssue, Global),
            (BillingPaymentView, Global),
            (BillingPaymentEdit, Global),
            (BillingPaymentIssue, Global),
            (TimesheetView, Global),
            (MailReceivedView, Global),
            (MailReceivedEdit, Global),
            (PayrollSlipView, Global),
            (PayrollSlipReview, Global),
            (ExpenseClaimView, Global),
            (ExpenseClaimReview, Global),
            (ScopeUnassigned, Global),
        ],
    },
    StandardRole {
        code: "hr",
        name: "人事",
        grants: &[
            (HrEmployeeView, Global),
            (HrEmployeeEdit, Global),
            (HrEmployeePrivateView, Global),
            (HrEmployeePrivateEdit, Global),
            (HrLeaveView, Global),
            (HrLeaveEdit, Global),
            (HrLeaveReview, Global),
            (ExpenseClaimView, Global),
            (ExpenseClaimReview, Global),
        ],
    },
    StandardRole {
        code: "legacy_payroll_all",
        name: "給与全件(移行用)",
        grants: &[(PayrollSlipView, Global), (HrEmployeeView, Global)],
    },
    StandardRole {
        code: "legacy_expense_all",
        name: "経費全件(移行用)",
        grants: &[
            (ExpenseClaimView, Global),
            (ExpenseClaimReview, Global),
            (ExpenseClaimApprove, Global),
        ],
    },
];

/// ロールコードから、その権限セットを返す。`admin` は `admin_grants()`、未知のコードは空。
pub fn grants_of(code: &str) -> Vec<(Permission, DataScope)> {
    if code == "admin" {
        return admin_grants();
    }
    STANDARD_ROLES
        .iter()
        .find(|r| r.code == code)
        .map(|r| r.grants.to_vec())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "standard_roles_tests.rs"]
mod tests;
