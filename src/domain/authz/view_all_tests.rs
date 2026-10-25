//! view_all.rs のテスト。

use std::sync::Arc;

use super::*;
use crate::domain::authz::decision::{AssignmentRow, RolePermissionRow};
use crate::domain::authz::org_tree::OrgTree;

fn authz_with(perms: &[(&str, &str)]) -> Authz {
    let tree = Arc::new(OrgTree::new(&[(1, None), (2, Some(1))]).unwrap());
    let role_perms: Vec<RolePermissionRow> = perms
        .iter()
        .map(|(c, s)| RolePermissionRow { role_id: 1, permission_code: (*c).into(), scope: (*s).into() })
        .collect();
    Authz::build(Some(10), false, &[AssignmentRow { org_unit_id: 1, role_id: 1 }], &role_perms, tree)
}

fn input(is_admin: bool, legacy_flag: bool, authz: Option<&Authz>) -> ViewAllInput<'_> {
    ViewAllInput { is_engineer: false, is_admin, legacy_flag, authz }
}

#[test]
fn 一般社員は本人分だけ() {
    assert!(!can_see_all(input(false, false, None), Permission::PayrollSlipView));
    let a = authz_with(&[]);
    assert!(!can_see_all(input(false, false, Some(&a)), Permission::PayrollSlipView));
}

#[test]
fn 管理者は全員分() {
    assert!(can_see_all(input(true, false, None), Permission::ExpenseClaimView));
}

#[test]
fn 既存スイッチは当面そのまま効く() {
    assert!(can_see_all(input(false, true, None), Permission::PayrollSlipView));
}

#[test]
fn ロールの権限を全社で持てば全員分() {
    let a = authz_with(&[("payroll.slip:view", "GLOBAL")]);
    assert!(can_see_all(input(false, false, Some(&a)), Permission::PayrollSlipView));
}

#[test]
fn 権限の種類が違えば全員分にならない() {
    // 経理の給与の権限では、社員情報(人事)の全員分は見えない
    let a = authz_with(&[("payroll.slip:view", "GLOBAL")]);
    assert!(!can_see_all(input(false, false, Some(&a)), Permission::HrEmployeeView));
}

#[test]
fn 全社でないスコープでは全員分にならない() {
    let a = authz_with(&[("expense.claim:view", "UNIT")]);
    assert!(!can_see_all(input(false, false, Some(&a)), Permission::ExpenseClaimView));
}

#[test]
fn 技術者は何を持っていても対象外() {
    let a = authz_with(&[("payroll.slip:view", "GLOBAL")]);
    let i = ViewAllInput { is_engineer: true, is_admin: true, legacy_flag: true, authz: Some(&a) };
    assert!(!can_see_all(i, Permission::PayrollSlipView));
}

#[test]
fn 給与明細は管理者だけでは全員分にならない() {
    // DEMO-000167 項目5: 給与は管理者の権限と分ける。スイッチかロールの payroll.slip:view(全社)が要る
    assert!(!can_see_all_payroll_slips(input(true, false, None)));
    assert!(can_see_all_payroll_slips(input(true, true, None)), "スイッチがあれば全員分");
    let a = authz_with(&[("payroll.slip:view", "GLOBAL")]);
    assert!(can_see_all_payroll_slips(input(false, false, Some(&a))), "ロールで全社なら全員分");
    let u = authz_with(&[("payroll.slip:view", "UNIT")]);
    assert!(!can_see_all_payroll_slips(input(false, false, Some(&u))), "全社でなければ全員分にならない");
    let e = ViewAllInput { is_engineer: true, is_admin: true, legacy_flag: true, authz: Some(&a) };
    assert!(!can_see_all_payroll_slips(e), "技術者は対象外");
}
