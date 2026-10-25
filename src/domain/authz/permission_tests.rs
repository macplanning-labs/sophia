//! permission.rs のテスト。
use super::Permission;
use std::collections::HashSet;

/// 権限コードの書式(小文字・'_' の英字、任意で "domain." と ":action")を満たすか。
fn is_valid_format(code: &str) -> bool {
    let (head, action) = match code.split_once(':') {
        Some((h, a)) => (h, Some(a)),
        None => (code, None),
    };
    let segment_ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c == '_');
    let head_ok = match head.split_once('.') {
        Some((a, b)) => segment_ok(a) && segment_ok(b),
        None => segment_ok(head),
    };
    head_ok && action.is_none_or(segment_ok)
}

#[test]
fn all_codes_are_unique() {
    let mut seen = HashSet::new();
    for p in Permission::ALL {
        assert!(seen.insert(p.code()), "duplicate code: {}", p.code());
    }
}

#[test]
fn code_round_trips_through_from_code() {
    for p in Permission::ALL {
        assert_eq!(Permission::from_code(p.code()), Some(*p));
    }
}

#[test]
fn unknown_code_is_none() {
    assert_eq!(Permission::from_code("contract:delete"), None);
    assert_eq!(Permission::from_code(""), None);
    assert_eq!(Permission::from_code("CONTRACT:VIEW"), None);
}

#[test]
fn all_has_58_entries() {
    assert_eq!(Permission::ALL.len(), 58);
}

#[test]
fn domain_is_prefix_before_separator() {
    assert_eq!(Permission::ContractPriceView.domain(), "contract");
    assert_eq!(Permission::ScopeUnassigned.domain(), "scope");
    assert_eq!(Permission::HrLeaveReview.domain(), "hr");
}

#[test]
fn is_sensitive_examples() {
    assert!(Permission::ContractPriceView.is_sensitive());
    assert!(Permission::PartnerBankEdit.is_sensitive());
    assert!(Permission::PayrollSlipView.is_sensitive());
    assert!(Permission::SystemOrgEdit.is_sensitive());
    assert!(!Permission::ContractView.is_sensitive());
    assert!(!Permission::ClientView.is_sensitive());
    assert!(!Permission::ExpenseClaimApprove.is_sensitive());
    assert!(!Permission::ScopeUnassigned.is_sensitive());
}

#[test]
fn is_stamped_examples() {
    assert!(Permission::ContractPriceView.is_stamped());
    assert!(Permission::ClientEdit.is_stamped());
    assert!(Permission::BillingPaymentIssue.is_stamped());
    assert!(Permission::TimesheetApprove.is_stamped());
    assert!(!Permission::PartnerView.is_stamped());
    assert!(!Permission::PartnerBankView.is_stamped());
    assert!(!Permission::EngineerEdit.is_stamped());
    assert!(!Permission::MailReceivedView.is_stamped());
    assert!(!Permission::PayrollSlipView.is_stamped());
    assert!(!Permission::ExpenseClaimReview.is_stamped());
    assert!(!Permission::ScopeUnassigned.is_stamped());
}

#[test]
fn all_codes_match_format() {
    for p in Permission::ALL {
        assert!(is_valid_format(p.code()), "invalid format: {}", p.code());
    }
    assert!(!is_valid_format("contract:delete:x"));
    assert!(!is_valid_format("Contract:view"));
}

#[test]
fn every_label_is_non_empty_and_unique() {
    let mut seen = HashSet::new();
    for p in Permission::ALL {
        let label = p.label_ja();
        assert!(!label.is_empty(), "empty label: {}", p.code());
        assert!(seen.insert(label), "duplicate label {label} ({})", p.code());
    }
}
