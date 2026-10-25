//! decision.rs / list_filter.rs のテスト(設計書 §7.4 の鈴木さんの表ほか)。

use super::*;
use crate::domain::authz::list_filter::{Clause, ListFilter};

const SUZUKI: i64 = 100;
const YAMADA: i64 = 200;
const TANAKA: i64 = 300;

/// 全社(1) > 事業部A(2) > {営業1課(3) > 1係(31), 営業2課(4)}, 事業部B(5) > 営業3課(6)
fn tree() -> Arc<OrgTree> {
    Arc::new(
        OrgTree::new(&[(1, None), (2, Some(1)), (3, Some(2)), (31, Some(3)), (4, Some(2)), (5, Some(1)), (6, Some(5))]).unwrap(),
    )
}

fn rp(role: i64, code: &str, scope: &str) -> RolePermissionRow {
    RolePermissionRow { role_id: role, permission_code: code.into(), scope: scope.into() }
}

fn asg(org: i64, role: i64) -> AssignmentRow {
    AssignmentRow { org_unit_id: org, role_id: role }
}

fn stamp(org: i64, emp: i64) -> OwnerStamp {
    OwnerStamp::new(org, emp)
}

const UNASSIGNED: OwnerStamp = OwnerStamp { org_unit_id: None, employee_id: None };

fn suzuki_roles() -> Vec<RolePermissionRow> {
    vec![rp(1, "contract:edit", "HIERARCHICAL"), rp(2, "contract:edit", "PRIVATE"), rp(9, "contract:edit", "GLOBAL")]
}

fn suzuki(assignments: &[AssignmentRow], roles: &[RolePermissionRow]) -> Authz {
    Authz::build(Some(SUZUKI), false, assignments, roles, tree())
}

fn check_suzuki(a: &Authz) {
    let p = Permission::ContractEdit;
    assert!(a.can(p, &stamp(3, YAMADA)), "課長として1課の山田");
    assert!(a.can(p, &stamp(31, YAMADA)), "1係は配下");
    assert!(a.can(p, &stamp(4, SUZUKI)), "応援として2課の自分");
    assert!(!a.can(p, &stamp(4, TANAKA)), "2課の他人は横漏れさせない");
    assert!(a.can(p, &stamp(3, SUZUKI)), "1課の自分");
    assert!(!a.can(p, &stamp(6, SUZUKI)), "別事業部は自分でも不可");
    assert!(!a.can(p, &stamp(2, YAMADA)), "親組織は不可");
    assert_eq!(a.require(p, &stamp(4, TANAKA)), Err(AuthzError::Forbidden));
    assert_eq!(a.require(p, &stamp(3, YAMADA)), Ok(()));
}

#[test]
fn suzuki_table() {
    check_suzuki(&suzuki(&[asg(3, 1), asg(4, 2)], &suzuki_roles()));
}

#[test]
fn suzuki_is_order_and_noise_independent() {
    check_suzuki(&suzuki(&[asg(4, 2), asg(3, 1)], &suzuki_roles()));
    let mut roles = suzuki_roles();
    roles.push(rp(7, "contract:view", "UNIT"));
    roles.push(rp(7, "order:edit", "PRIVATE"));
    check_suzuki(&suzuki(&[asg(3, 1), asg(6, 7), asg(4, 2)], &roles));
}

fn single(scope: &str) -> Authz {
    suzuki(&[asg(3, 1)], &[rp(1, "contract:view", scope)])
}

#[test]
fn scope_table() {
    // (スタンプ, [PRIVATE, UNIT, HIERARCHICAL, GLOBAL])
    let cases = [
        ("org3 自分", stamp(3, SUZUKI), [true, true, true, true]),
        ("org3 他人", stamp(3, YAMADA), [false, true, true, true]),
        ("org31 他人(子)", stamp(31, YAMADA), [false, false, true, true]),
        ("org31 自分(子)", stamp(31, SUZUKI), [true, false, true, true]),
        ("org2 他人(親)", stamp(2, YAMADA), [false, false, false, true]),
        ("org4 他人(兄弟)", stamp(4, YAMADA), [false, false, false, true]),
        ("org4 自分(兄弟)", stamp(4, SUZUKI), [false, false, false, true]),
        ("未設定", UNASSIGNED, [false, false, false, true]),
    ];
    for (name, st, expect) in cases {
        for (i, sc) in ["PRIVATE", "UNIT", "HIERARCHICAL", "GLOBAL"].iter().enumerate() {
            let a = single(sc);
            assert_eq!(a.can(Permission::ContractView, &st), expect[i], "{name} / {sc}");
            // 一覧条件(SQL 相当)と判定が一致すること
            assert_eq!(a.list_filter(Permission::ContractView).matches(&st), expect[i], "filter {name} / {sc}");
        }
    }
}

#[test]
fn can_and_filter_agree_for_suzuki() {
    let a = suzuki(&[asg(3, 1), asg(4, 2)], &suzuki_roles());
    let f = a.list_filter(Permission::ContractEdit);
    for org in [1, 2, 3, 31, 4, 5, 6] {
        for emp in [SUZUKI, YAMADA, TANAKA] {
            let st = stamp(org, emp);
            assert_eq!(a.can(Permission::ContractEdit, &st), f.matches(&st), "org {org} emp {emp}");
        }
    }
    assert!(f.matches(&stamp(4, SUZUKI)) && !f.matches(&stamp(4, TANAKA)));
}

#[test]
fn unassigned_rule() {
    let roles = vec![
        rp(1, "contract:view", "UNIT"),
        rp(1, "scope.unassigned", "GLOBAL"),
        rp(2, "contract:view", "UNIT"),
        rp(3, "scope.unassigned", "GLOBAL"),
    ];
    let with = suzuki(&[asg(3, 1)], &roles);
    let without = suzuki(&[asg(3, 2)], &roles);
    let split = suzuki(&[asg(3, 2), asg(4, 3)], &roles);

    if !UNASSIGNED_RULE_ENABLED {
        // PR 7b の後(担当は必須): scope.unassigned を持っていても、担当未設定のデータは誰にも見えない
        for a in [&with, &without, &split] {
            assert!(!a.can(Permission::ContractView, &UNASSIGNED));
            assert!(!a.list_filter(Permission::ContractView).matches(&UNASSIGNED));
            assert!(!a.list_filter(Permission::ContractView).0.contains(&Clause::Unassigned));
        }
        // 担当のあるデータの判定は変わらない
        assert!(with.can(Permission::ContractView, &stamp(3, SUZUKI)));
        return;
    }

    assert!(with.can(Permission::ContractView, &UNASSIGNED));
    assert!(with.list_filter(Permission::ContractView).matches(&UNASSIGNED));
    assert!(with.list_filter(Permission::ContractView).0.contains(&Clause::Unassigned));

    assert!(!without.can(Permission::ContractView, &UNASSIGNED));
    assert!(!without.list_filter(Permission::ContractView).matches(&UNASSIGNED));

    // 別の行(ロールA = 閲覧のみ、ロールB = 未設定のみ)では許さない
    assert!(!split.can(Permission::ContractView, &UNASSIGNED));
    assert!(split.list_filter(Permission::ContractView).is_empty() || !split.list_filter(Permission::ContractView).matches(&UNASSIGNED));
}

#[test]
fn unknown_code_and_scope_are_ignored() {
    let roles = vec![rp(1, "contract:fly", "GLOBAL"), rp(1, "contract:view", "EVERYTHING"), rp(1, "contract:edit", "UNIT")];
    let a = suzuki(&[asg(3, 1)], &roles);
    assert!(!a.has_any(Permission::ContractView));
    assert!(!a.can(Permission::ContractView, &stamp(3, SUZUKI)));
    assert!(a.can(Permission::ContractEdit, &stamp(3, YAMADA)));
}

#[test]
fn unstamped_permissions_ignore_org() {
    let roles = vec![rp(1, "partner.master:view", "UNIT"), rp(1, "mail.received:view", "PRIVATE")];
    let a = suzuki(&[asg(3, 1)], &roles);
    assert!(a.can(Permission::PartnerView, &stamp(6, TANAKA)));
    assert!(a.can(Permission::MailReceivedView, &UNASSIGNED));
    assert!(a.can_unstamped(Permission::PartnerView));
    assert!(a.list_filter(Permission::PartnerView).allows_all());
    let none = suzuki(&[asg(3, 1)], &[]);
    assert!(!none.can(Permission::PartnerView, &stamp(3, SUZUKI)));
    assert!(none.list_filter(Permission::PartnerView).is_empty());
}

#[test]
fn staff_can_everything() {
    let a = Authz::build(None, true, &[], &[], tree());
    assert!(a.is_staff());
    assert!(a.can(Permission::ContractEdit, &UNASSIGNED));
    assert!(a.can(Permission::PartnerView, &stamp(6, 1)));
    assert_eq!(a.list_filter(Permission::ContractEdit), ListFilter(vec![Clause::All]));
}

#[test]
fn private_without_employee_denies() {
    let a = Authz::build(None, false, &[asg(3, 2)], &suzuki_roles(), tree());
    assert!(!a.can(Permission::ContractEdit, &stamp(3, SUZUKI)));
    assert!(a.list_filter(Permission::ContractEdit).is_empty());
}

#[test]
fn can_stamp_new_requires_complete_and_in_range() {
    let a = suzuki(&[asg(3, 1)], &suzuki_roles());
    assert!(a.can_stamp_new(Permission::ContractEdit, &stamp(31, YAMADA)));
    assert!(!a.can_stamp_new(Permission::ContractEdit, &stamp(4, YAMADA)));
    assert!(!a.can_stamp_new(Permission::ContractEdit, &OwnerStamp { org_unit_id: Some(3), employee_id: None }));
}

#[test]
fn validate_assignment_rules() {
    let t = tree();
    let global_stamped = vec![rp(1, "contract:view", "GLOBAL")];
    assert_eq!(validate_assignment(&global_stamped, 3, &t), Err(AssignmentError::GlobalStampedOutsideRoot));
    assert_eq!(validate_assignment(&global_stamped, 1, &t), Ok(()));
    let global_master = vec![rp(1, "partner.master:view", "GLOBAL"), rp(1, "contract:view", "UNIT")];
    assert_eq!(validate_assignment(&global_master, 3, &t), Ok(()));
    assert_eq!(validate_assignment(&global_master, 99, &t), Err(AssignmentError::UnknownOrg));
}

#[test]
fn role_permission_edits_revalidate_existing_assignments() {
    let t = tree();
    // 課(3)に割り当て済みのロールへ、スタンプ型の GLOBAL を足すのは拒否(既存の割り当てが全社を見てしまう)
    let edited = [rp(1, "contract:view", "GLOBAL")];
    assert_eq!(validate_role_permissions(&edited, &[3], &t), Err(AssignmentError::GlobalStampedOutsideRoot));
    assert_eq!(validate_role_permissions(&edited, &[1], &t), Ok(()));
    // スタンプを持たない権限は GLOBAL 以外を付けられない(黙って無視されるのを防ぐ)
    let private_expense = [rp(1, "expense.claim:view", "PRIVATE")];
    assert_eq!(
        validate_role_permissions(&private_expense, &[], &t),
        Err(AssignmentError::NonGlobalScopeOnUnstamped("expense.claim:view".to_string()))
    );
    // 未知のコード・スコープは保存させない
    assert!(matches!(validate_role_permissions(&[rp(1, "contract:delete", "GLOBAL")], &[], &t), Err(AssignmentError::UnknownCodeOrScope(_))));
    assert!(matches!(validate_role_permissions(&[rp(1, "contract:view", "ALL")], &[], &t), Err(AssignmentError::UnknownCodeOrScope(_))));
}

#[test]
fn standard_roles_pass_role_permission_validation() {
    // 標準ロールの初期値そのものが、保存時の検証を通ること(経理・管理者はルート、その他は課に割り当てる想定)
    use crate::domain::authz::standard_roles::{grants_of, STANDARD_ROLES};
    let t = tree();
    for r in STANDARD_ROLES {
        let rows: Vec<RolePermissionRow> = grants_of(r.code)
            .into_iter()
            .map(|(p, s)| RolePermissionRow { role_id: 1, permission_code: p.code().to_string(), scope: s.to_db().to_string() })
            .collect();
        let org = if matches!(r.code, "admin" | "accounting" | "hr" | "legacy_payroll_all" | "legacy_expense_all") { 1 } else { 3 };
        assert_eq!(validate_role_permissions(&rows, &[org], &t), Ok(()), "{}", r.code);
    }
}
