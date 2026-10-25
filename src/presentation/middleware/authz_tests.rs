//! authz.rs の単体テスト(DB なし)。

use super::*;
use crate::presentation::route_table::ROUTE_PERMISSIONS;

fn cfg(mode: AuthzMode, domains: &[&str]) -> AuthzConfig {
    AuthzConfig { mode, enforce_domains: domains.iter().map(|s| s.to_string()).collect() }
}

#[test]
fn config_defaults_to_shadow() {
    assert_eq!(AuthzConfig::from_values(None, None), cfg(AuthzMode::Shadow, &[]));
    assert_eq!(AuthzConfig::from_values(Some(""), None).mode, AuthzMode::Shadow);
}

#[test]
fn config_mode_is_case_insensitive_and_unknown_is_shadow() {
    assert_eq!(AuthzConfig::from_values(Some("OFF"), None).mode, AuthzMode::Off);
    assert_eq!(AuthzConfig::from_values(Some(" Enforce "), None).mode, AuthzMode::Enforce);
    assert_eq!(AuthzConfig::from_values(Some("SHADOW"), None).mode, AuthzMode::Shadow);
    assert_eq!(AuthzConfig::from_values(Some("bogus"), None).mode, AuthzMode::Shadow);
}

#[test]
fn config_domains_are_trimmed_and_empties_dropped() {
    let c = AuthzConfig::from_values(Some("enforce"), Some(" settings, partner ,, "));
    assert_eq!(c.enforce_domains, vec!["settings".to_string(), "partner".to_string()]);
}

#[test]
fn entry_decision_all_rules() {
    let p = Permission::EngineerEdit;
    for is_admin in [false, true] {
        for has in [false, true] {
            let d = |rule| entry_decision(Some(rule), is_admin, |_| has);
            assert_eq!(d(Rule::Perm(p)), if is_admin || has { EntryDecision::Allow } else { EntryDecision::Deny });
            assert_eq!(d(Rule::AdminOnly), if is_admin { EntryDecision::Allow } else { EntryDecision::Deny });
            assert_eq!(d(Rule::OwnOr(p)), EntryDecision::Allow);
            assert_eq!(d(Rule::Own), EntryDecision::Allow);
            assert_eq!(d(Rule::Authenticated), EntryDecision::Allow);
            assert_eq!(entry_decision(None, is_admin, |_| has), EntryDecision::Deny);
        }
    }
}

#[test]
fn outcome_off_passes_everything() {
    let c = cfg(AuthzMode::Off, &["*"]);
    assert_eq!(outcome(&c, None, EntryDecision::Deny), Outcome::Pass);
    assert_eq!(outcome(&c, Some(Rule::AdminOnly), EntryDecision::Deny), Outcome::Pass);
}

#[test]
fn outcome_allow_always_passes() {
    assert_eq!(outcome(&cfg(AuthzMode::Enforce, &["*"]), Some(Rule::Own), EntryDecision::Allow), Outcome::Pass);
}

#[test]
fn outcome_enforce_only_enforced_domains() {
    let p = Permission::EngineerEdit;
    let rule = Some(Rule::Perm(p));
    let on = cfg(AuthzMode::Enforce, &[p.domain()]);
    let other = cfg(AuthzMode::Enforce, &["no_such_domain"]);
    assert_eq!(outcome(&on, rule, EntryDecision::Deny), Outcome::Reject);
    assert_eq!(outcome(&other, rule, EntryDecision::Deny), Outcome::PassButLogDiff);
    assert_eq!(outcome(&cfg(AuthzMode::Enforce, &["admin_only"]), Some(Rule::AdminOnly), EntryDecision::Deny), Outcome::Reject);
    // 権限コードの system ドメインを強制しても、管理者専用ルートは強制されない(取り違え防止)
    assert_eq!(outcome(&cfg(AuthzMode::Enforce, &["system"]), Some(Rule::AdminOnly), EntryDecision::Deny), Outcome::PassButLogDiff);
    assert_eq!(outcome(&other, Some(Rule::AdminOnly), EntryDecision::Deny), Outcome::PassButLogDiff);
}

#[test]
fn outcome_missing_route_rejected_only_with_star() {
    assert_eq!(outcome(&cfg(AuthzMode::Enforce, &["settings"]), None, EntryDecision::Deny), Outcome::PassButLogDiff);
    assert_eq!(outcome(&cfg(AuthzMode::Enforce, &["*"]), None, EntryDecision::Deny), Outcome::Reject);
}

/// シャドーモードは挙動を変えない: 権限ゼロのユーザーでも、表の全ルートで Reject にならない。
#[test]
fn shadow_mode_never_rejects_any_route() {
    let c = cfg(AuthzMode::Shadow, &["*", "system", "settings"]);
    for spec in ROUTE_PERMISSIONS {
        let d = entry_decision(Some(spec.rule), false, |_| false);
        assert_ne!(outcome(&c, Some(spec.rule), d), Outcome::Reject, "{} {}", spec.method, spec.path);
    }
    assert_ne!(outcome(&c, None, EntryDecision::Deny), Outcome::Reject);
}
