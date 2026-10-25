//! standard_roles.rs のテスト。
use crate::domain::authz::permission::Permission;
use crate::domain::authz::scope::DataScope;
use super::*;
use std::collections::{BTreeSet, HashSet};

/// 指定ロールが、指定の権限を(どのスコープでも)持つか。
fn holds(code: &str, p: Permission) -> bool {
    grants_of(code).iter().any(|(q, _)| *q == p)
}

/// 指定の権限を持つロールコードの集合(admin を含む)。
fn holders(p: Permission) -> BTreeSet<&'static str> {
    STANDARD_ROLES
        .iter()
        .map(|r| r.code)
        .filter(|c| holds(c, p))
        .collect()
}

fn set(codes: &[&'static str]) -> BTreeSet<&'static str> {
    codes.iter().copied().collect()
}

#[test]
fn role_codes_are_unique_and_there_are_seven() {
    // 標準ロールは 7 件で、コードは重複しない
    assert_eq!(STANDARD_ROLES.len(), 7);
    let mut seen = HashSet::new();
    for r in STANDARD_ROLES {
        assert!(seen.insert(r.code), "duplicate role code: {}", r.code);
    }
}

#[test]
fn no_duplicate_permission_inside_a_role() {
    // 同じロールに同じ権限を二重に入れない(admin を含む)
    for r in STANDARD_ROLES {
        let grants = grants_of(r.code);
        let mut seen = HashSet::new();
        for (p, _) in &grants {
            assert!(seen.insert(*p), "role {} has duplicate {}", r.code, p.code());
        }
    }
}

#[test]
fn hr_has_no_payroll_permission() {
    // U8: 人事は給与の金額を見ない・確認もしない(payroll.* を一切持たない)
    for (p, _) in grants_of("hr") {
        assert!(!p.code().starts_with("payroll."), "hr must not hold {}", p.code());
    }
}

#[test]
fn expense_approve_and_payroll_approve_are_restricted() {
    // D4: 経費の承認は admin と legacy_expense_all だけ
    assert_eq!(holders(ExpenseClaimApprove), set(&["admin", "legacy_expense_all"]));
    // D12: 給与の承認は admin だけ。経理は給与の確認(review)を持つ
    assert_eq!(holders(PayrollSlipApprove), set(&["admin"]));
    assert!(holds("accounting", PayrollSlipReview));
    // D12: 人事は給与の確認を持たない
    assert!(!holds("hr", PayrollSlipReview));
    // 経費の確認は人事と経理が持つ
    assert!(holds("hr", ExpenseClaimReview));
    assert!(holds("accounting", ExpenseClaimReview));
}

#[test]
fn partner_bank_only_for_admin_and_accounting() {
    // D3/D14: 取引先の振込口座は admin と経理だけが見る・編集する
    let expected = set(&["admin", "accounting"]);
    assert_eq!(holders(PartnerBankView), expected);
    assert_eq!(holders(PartnerBankEdit), expected);
}

#[test]
fn contract_price_view_holders() {
    // D2: 契約単価の閲覧は admin・営業責任者・営業担当・経理。人事と legacy_* は持たない
    assert_eq!(
        holders(ContractPriceView),
        set(&["admin", "sales_lead", "sales", "accounting"])
    );
}

#[test]
fn review_implies_view_in_every_role() {
    // ねじれ禁止: 確認(review)を持つロールは、対応する閲覧(view)も持つ
    for r in STANDARD_ROLES {
        let grants = grants_of(r.code);
        let has = |p: Permission| grants.iter().any(|(q, _)| *q == p);
        if has(ExpenseClaimReview) {
            assert!(has(ExpenseClaimView), "{} has ExpenseClaimReview without ExpenseClaimView", r.code);
        }
        if has(PayrollSlipReview) {
            assert!(has(PayrollSlipView), "{} has PayrollSlipReview without PayrollSlipView", r.code);
        }
    }
}

#[test]
fn stamped_global_only_for_root_org_roles() {
    // 担当スタンプ付きのデータを全社(GLOBAL)で持てるのは、ルート組織のロール(admin・経理)だけ
    let allowed = set(&["admin", "accounting"]);
    for r in STANDARD_ROLES {
        for (p, scope) in grants_of(r.code) {
            if p.is_stamped() && scope == DataScope::Global {
                assert!(
                    allowed.contains(r.code),
                    "role {} holds stamped {} with GLOBAL",
                    r.code,
                    p.code()
                );
            }
        }
    }
}

#[test]
fn sales_and_sales_lead_have_same_permissions_with_different_scopes() {
    // sales と sales_lead は同じ権限を持つ。担当スタンプ付きは sales_lead が組織配下(H)、
    // sales が本人限定(PRIVATE)。それ以外は両方とも全社(G)
    let lead = grants_of("sales_lead");
    let sales = grants_of("sales");
    let lead_perms: BTreeSet<&str> = lead.iter().map(|(p, _)| p.code()).collect();
    let sales_perms: BTreeSet<&str> = sales.iter().map(|(p, _)| p.code()).collect();
    assert_eq!(lead_perms, sales_perms);
    for (p, scope) in &lead {
        let sales_scope = sales.iter().find(|(q, _)| q == p).map(|(_, s)| *s);
        if p.is_stamped() {
            assert_eq!(*scope, DataScope::Hierarchical, "sales_lead {}", p.code());
            assert_eq!(sales_scope, Some(DataScope::Private), "sales {}", p.code());
        } else {
            assert_eq!(*scope, DataScope::Global, "sales_lead {}", p.code());
            assert_eq!(sales_scope, Some(DataScope::Global), "sales {}", p.code());
        }
    }
}

#[test]
fn admin_grants_cover_every_permission_exactly_once() {
    // 管理者は全権限を一度ずつ、全社(GLOBAL)で持つ
    let grants = admin_grants();
    assert_eq!(grants.len(), Permission::ALL.len());
    let mut seen = HashSet::new();
    for (p, scope) in &grants {
        assert!(seen.insert(*p), "admin has duplicate {}", p.code());
        assert_eq!(*scope, DataScope::Global);
    }
    for p in Permission::ALL {
        assert!(seen.contains(p), "admin missing {}", p.code());
    }
}

#[test]
fn scope_unassigned_holders() {
    // 担当未割当データの閲覧は、admin・営業責任者・営業担当・経理が持つ。人事は持たない
    assert_eq!(
        holders(ScopeUnassigned),
        set(&["admin", "sales_lead", "sales", "accounting"])
    );
    assert!(!holds("hr", ScopeUnassigned));
}
