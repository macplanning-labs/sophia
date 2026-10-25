//! domain/authz/view_all.rs — 「全員分を見てよいか」の判定(DEMO-000173 PR 8)。
//!
//! 給与・社員情報・経費は担当スタンプを持たない領域で、「全員分」か「本人分だけ」かの二択になる。
//! 全員分を見てよいのは次のどれか(どれか1つで足りる)。ただし**給与明細だけは 1 を数えない**(`can_see_all_payroll_slips`)。
//! 1. 管理者(is_staff)
//! 2. 既存のスイッチ(`can_view_all_payroll` / `can_view_all_expenses`)。PR 10 で整理するまで残す
//! 3. ロールの権限を全社(GLOBAL)で持つ(給与=`payroll.slip:view`、社員情報=`hr.employee:view`、経費=`expense.claim:view`)
//!
//! 技術者の直接認証(`m_engineer`)は、どれを持っていても常に対象外(従来どおり)。
//! DB には触れない純粋な関数。

use super::decision::Authz;
use super::permission::Permission;

/// 「全員分を見てよいか」の入力。呼び出し者の状態をまとめる。
#[derive(Debug, Clone, Copy)]
pub struct ViewAllInput<'a> {
    /// 技術者の直接認証か(true なら常に不可)。
    pub is_engineer: bool,
    /// 管理者(is_staff)か。
    pub is_admin: bool,
    /// 既存スイッチ(給与なら `can_view_all_payroll`、経費なら `can_view_all_expenses`)の値。
    pub legacy_flag: bool,
    /// ロールから組み立てた権限。読み込めなかった・管理者・入口で読まれなかった場合は `None`。
    pub authz: Option<&'a Authz>,
}

/// `perm` について、全員分を見てよいか。
pub fn can_see_all(input: ViewAllInput<'_>, perm: Permission) -> bool {
    if input.is_engineer {
        return false;
    }
    input.is_admin || input.legacy_flag || input.authz.is_some_and(|a| a.has_global(perm))
}

/// 給与明細の全員分を見てよいか。**管理者であることだけでは足りない**(DEMO-000167 項目5の決まり)。
/// 給与は、管理者の権限とは分けて、給与の全件スイッチか、ロールの `payroll.slip:view`(全社)で決める。
/// 例: 給与のスイッチを持たない管理者は、自分の明細だけ。
pub fn can_see_all_payroll_slips(input: ViewAllInput<'_>) -> bool {
    if input.is_engineer {
        return false;
    }
    input.legacy_flag || input.authz.is_some_and(|a| a.has_global_grant(Permission::PayrollSlipView))
}

#[cfg(test)]
#[path = "view_all_tests.rs"]
mod tests;
