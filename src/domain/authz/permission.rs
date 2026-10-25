//! domain/authz/permission.rs — 権限コードの一覧。
//!
//! 権限コードの一覧はコードで持ち、どのロールが持つかは DB(m_role_permission)で持つ。
//! DB にあってここに無いコードは無視して拒否側に倒す。

/// 権限コード。文字列の正本はこの enum の `code()` にある。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    HrEmployeeView,
    HrEmployeeEdit,
    HrEmployeePrivateView,
    HrEmployeePrivateEdit,
    HrLeaveView,
    HrLeaveEdit,
    HrLeaveReview,
    PayrollSlipView,
    PayrollSlipEdit,
    PayrollSlipReview,
    PayrollSlipApprove,
    PayrollSlipIssue,
    PayrollMasterView,
    PayrollMasterEdit,
    ExpenseClaimView,
    ExpenseClaimEdit,
    ExpenseClaimReview,
    ExpenseClaimApprove,
    ClientView,
    ClientEdit,
    PartnerView,
    PartnerEdit,
    PartnerBankView,
    PartnerBankEdit,
    EngineerView,
    EngineerEdit,
    ProjectView,
    ProjectEdit,
    ContractView,
    ContractEdit,
    ContractPriceView,
    ContractPriceEdit,
    OrderPurchaseView,
    OrderPurchaseEdit,
    OrderPurchaseIssue,
    OrderReceivedView,
    OrderReceivedEdit,
    TimesheetView,
    TimesheetEdit,
    TimesheetApprove,
    BillingInvoiceView,
    BillingInvoiceEdit,
    BillingInvoiceIssue,
    BillingPaymentView,
    BillingPaymentEdit,
    BillingPaymentIssue,
    MailReceivedView,
    MailReceivedEdit,
    MailSendIssue,
    SettingsCompanyView,
    SettingsCompanyEdit,
    SystemUserView,
    SystemUserEdit,
    SystemOrgView,
    SystemOrgEdit,
    SystemAuditView,
    SystemAuditEdit,
    ScopeUnassigned,
}

impl Permission {
    /// 全権限コードの一覧(定義順)。
    pub const ALL: &'static [Permission] = &[
        Permission::HrEmployeeView,
        Permission::HrEmployeeEdit,
        Permission::HrEmployeePrivateView,
        Permission::HrEmployeePrivateEdit,
        Permission::HrLeaveView,
        Permission::HrLeaveEdit,
        Permission::HrLeaveReview,
        Permission::PayrollSlipView,
        Permission::PayrollSlipEdit,
        Permission::PayrollSlipReview,
        Permission::PayrollSlipApprove,
        Permission::PayrollSlipIssue,
        Permission::PayrollMasterView,
        Permission::PayrollMasterEdit,
        Permission::ExpenseClaimView,
        Permission::ExpenseClaimEdit,
        Permission::ExpenseClaimReview,
        Permission::ExpenseClaimApprove,
        Permission::ClientView,
        Permission::ClientEdit,
        Permission::PartnerView,
        Permission::PartnerEdit,
        Permission::PartnerBankView,
        Permission::PartnerBankEdit,
        Permission::EngineerView,
        Permission::EngineerEdit,
        Permission::ProjectView,
        Permission::ProjectEdit,
        Permission::ContractView,
        Permission::ContractEdit,
        Permission::ContractPriceView,
        Permission::ContractPriceEdit,
        Permission::OrderPurchaseView,
        Permission::OrderPurchaseEdit,
        Permission::OrderPurchaseIssue,
        Permission::OrderReceivedView,
        Permission::OrderReceivedEdit,
        Permission::TimesheetView,
        Permission::TimesheetEdit,
        Permission::TimesheetApprove,
        Permission::BillingInvoiceView,
        Permission::BillingInvoiceEdit,
        Permission::BillingInvoiceIssue,
        Permission::BillingPaymentView,
        Permission::BillingPaymentEdit,
        Permission::BillingPaymentIssue,
        Permission::MailReceivedView,
        Permission::MailReceivedEdit,
        Permission::MailSendIssue,
        Permission::SettingsCompanyView,
        Permission::SettingsCompanyEdit,
        Permission::SystemUserView,
        Permission::SystemUserEdit,
        Permission::SystemOrgView,
        Permission::SystemOrgEdit,
        Permission::SystemAuditView,
        Permission::SystemAuditEdit,
        Permission::ScopeUnassigned,
    ];

    /// 権限コード文字列(DB の `m_role_permission.permission_code` と一致する値)を返す。
    pub fn code(self) -> &'static str {
        match self {
            Permission::HrEmployeeView => "hr.employee:view",
            Permission::HrEmployeeEdit => "hr.employee:edit",
            Permission::HrEmployeePrivateView => "hr.employee_private:view",
            Permission::HrEmployeePrivateEdit => "hr.employee_private:edit",
            Permission::HrLeaveView => "hr.leave:view",
            Permission::HrLeaveEdit => "hr.leave:edit",
            Permission::HrLeaveReview => "hr.leave:review",
            Permission::PayrollSlipView => "payroll.slip:view",
            Permission::PayrollSlipEdit => "payroll.slip:edit",
            Permission::PayrollSlipReview => "payroll.slip:review",
            Permission::PayrollSlipApprove => "payroll.slip:approve",
            Permission::PayrollSlipIssue => "payroll.slip:issue",
            Permission::PayrollMasterView => "payroll.master:view",
            Permission::PayrollMasterEdit => "payroll.master:edit",
            Permission::ExpenseClaimView => "expense.claim:view",
            Permission::ExpenseClaimEdit => "expense.claim:edit",
            Permission::ExpenseClaimReview => "expense.claim:review",
            Permission::ExpenseClaimApprove => "expense.claim:approve",
            Permission::ClientView => "client.master:view",
            Permission::ClientEdit => "client.master:edit",
            Permission::PartnerView => "partner.master:view",
            Permission::PartnerEdit => "partner.master:edit",
            Permission::PartnerBankView => "partner.bank:view",
            Permission::PartnerBankEdit => "partner.bank:edit",
            Permission::EngineerView => "engineer.master:view",
            Permission::EngineerEdit => "engineer.master:edit",
            Permission::ProjectView => "project.master:view",
            Permission::ProjectEdit => "project.master:edit",
            Permission::ContractView => "contract:view",
            Permission::ContractEdit => "contract:edit",
            Permission::ContractPriceView => "contract.price:view",
            Permission::ContractPriceEdit => "contract.price:edit",
            Permission::OrderPurchaseView => "order.purchase:view",
            Permission::OrderPurchaseEdit => "order.purchase:edit",
            Permission::OrderPurchaseIssue => "order.purchase:issue",
            Permission::OrderReceivedView => "order.received:view",
            Permission::OrderReceivedEdit => "order.received:edit",
            Permission::TimesheetView => "timesheet:view",
            Permission::TimesheetEdit => "timesheet:edit",
            Permission::TimesheetApprove => "timesheet:approve",
            Permission::BillingInvoiceView => "billing.invoice:view",
            Permission::BillingInvoiceEdit => "billing.invoice:edit",
            Permission::BillingInvoiceIssue => "billing.invoice:issue",
            Permission::BillingPaymentView => "billing.payment:view",
            Permission::BillingPaymentEdit => "billing.payment:edit",
            Permission::BillingPaymentIssue => "billing.payment:issue",
            Permission::MailReceivedView => "mail.received:view",
            Permission::MailReceivedEdit => "mail.received:edit",
            Permission::MailSendIssue => "mail.send:issue",
            Permission::SettingsCompanyView => "settings.company:view",
            Permission::SettingsCompanyEdit => "settings.company:edit",
            Permission::SystemUserView => "system.user:view",
            Permission::SystemUserEdit => "system.user:edit",
            Permission::SystemOrgView => "system.org:view",
            Permission::SystemOrgEdit => "system.org:edit",
            Permission::SystemAuditView => "system.audit:view",
            Permission::SystemAuditEdit => "system.audit:edit",
            Permission::ScopeUnassigned => "scope.unassigned",
        }
    }

    /// 権限コード文字列から `Permission` を引く。一覧に無いコードは `None`(拒否側に倒す)。
    pub fn from_code(code: &str) -> Option<Permission> {
        Self::ALL.iter().copied().find(|p| p.code() == code)
    }

    /// 管理画面に出す短い日本語名。
    pub fn label_ja(self) -> &'static str {
        match self {
            Permission::HrEmployeeView => "社員情報を見る",
            Permission::HrEmployeeEdit => "社員情報を編集する",
            Permission::HrEmployeePrivateView => "社員の個人情報を見る",
            Permission::HrEmployeePrivateEdit => "社員の個人情報を編集する",
            Permission::HrLeaveView => "休暇を見る",
            Permission::HrLeaveEdit => "休暇を編集する",
            Permission::HrLeaveReview => "休暇を確認する",
            Permission::PayrollSlipView => "給与明細を見る",
            Permission::PayrollSlipEdit => "給与明細を編集する",
            Permission::PayrollSlipReview => "給与明細を確認する",
            Permission::PayrollSlipApprove => "給与明細を承認する",
            Permission::PayrollSlipIssue => "給与明細を発行する",
            Permission::PayrollMasterView => "給与マスタを見る",
            Permission::PayrollMasterEdit => "給与マスタを編集する",
            Permission::ExpenseClaimView => "経費申請を見る",
            Permission::ExpenseClaimEdit => "経費申請を編集する",
            Permission::ExpenseClaimReview => "経費申請を確認する",
            Permission::ExpenseClaimApprove => "経費申請を承認する",
            Permission::ClientView => "取引先を見る",
            Permission::ClientEdit => "取引先を編集する",
            Permission::PartnerView => "パートナーを見る",
            Permission::PartnerEdit => "パートナーを編集する",
            Permission::PartnerBankView => "パートナーの口座を見る",
            Permission::PartnerBankEdit => "パートナーの口座を編集する",
            Permission::EngineerView => "要員を見る",
            Permission::EngineerEdit => "要員を編集する",
            Permission::ProjectView => "案件を見る",
            Permission::ProjectEdit => "案件を編集する",
            Permission::ContractView => "契約を見る",
            Permission::ContractEdit => "契約を編集する",
            Permission::ContractPriceView => "契約の単価を見る",
            Permission::ContractPriceEdit => "契約の単価を編集する",
            Permission::OrderPurchaseView => "発注書を見る",
            Permission::OrderPurchaseEdit => "発注書を編集する",
            Permission::OrderPurchaseIssue => "発注書を発行する",
            Permission::OrderReceivedView => "受注書を見る",
            Permission::OrderReceivedEdit => "受注書を編集する",
            Permission::TimesheetView => "勤務表を見る",
            Permission::TimesheetEdit => "勤務表を編集する",
            Permission::TimesheetApprove => "勤務表を承認する",
            Permission::BillingInvoiceView => "請求書を見る",
            Permission::BillingInvoiceEdit => "請求書を編集する",
            Permission::BillingInvoiceIssue => "請求書を発行する",
            Permission::BillingPaymentView => "支払通知を見る",
            Permission::BillingPaymentEdit => "支払通知を編集する",
            Permission::BillingPaymentIssue => "支払通知を発行する",
            Permission::MailReceivedView => "受信メールを見る",
            Permission::MailReceivedEdit => "受信メールを編集する",
            Permission::MailSendIssue => "メールを送信する",
            Permission::SettingsCompanyView => "会社設定を見る",
            Permission::SettingsCompanyEdit => "会社設定を編集する",
            Permission::SystemUserView => "ユーザーを見る",
            Permission::SystemUserEdit => "ユーザーを編集する",
            Permission::SystemOrgView => "組織とロールを見る",
            Permission::SystemOrgEdit => "組織とロールを編集する",
            Permission::SystemAuditView => "監査ログを見る",
            Permission::SystemAuditEdit => "監査設定を編集する",
            Permission::ScopeUnassigned => "担当未設定のデータも扱う",
        }
    }

    /// 権限コードの先頭部分(最初の '.' または ':' より前)を返す。例: "contract.price:view" -> "contract"。
    pub fn domain(self) -> &'static str {
        let code = self.code();
        match code.find(['.', ':']) {
            Some(i) => &code[..i],
            None => code,
        }
    }

    /// 担当スタンプ(担当組織・担当者)を持つデータの権限か(設計書 §6.5)。
    /// スタンプ型の権限だけが、スコープ(本人 / 自組織 / 組織配下 / 全社)で範囲を絞る。
    /// それ以外(マスタ・受信メール・個人データ・設定・システム)は、権限があるかだけで判定する(§6.2 ④)。
    pub fn is_stamped(self) -> bool {
        matches!(self.domain(), "client" | "project" | "contract" | "order" | "billing" | "timesheet")
    }

    /// 機微度が高い権限(設計書 §3・§8.5)。ルート表で「ログインだけで可」にしてはいけない。
    pub fn is_sensitive(self) -> bool {
        let code = self.code();
        [
            "contract.price",
            "partner.bank",
            "payroll.",
            "hr.employee_private",
            "settings.",
            "system.",
        ]
        .iter()
        .any(|prefix| code.starts_with(prefix))
    }
}

#[cfg(test)]
#[path = "permission_tests.rs"]
mod tests;
