//! domain/mfa_policy.rs — MFA必須化・最終要素保護のポリシーフック（方針ドキュメント5章）
//!
//! 判定フック（「最後の1要素なら無効化を拒否する」という流れ）はauth-core、
//! 判定内容（「誰に必須か」）は各アプリが`MfaPolicy`を実装して注入する。

pub trait MfaPolicy: Send + Sync {
    /// このロール集合を持つユーザーにMFAを必須とするか
    fn requires_mfa(&self, roles: &[String]) -> bool;

    /// 「最後の1つのMFA要素」を無効化しようとした場合に許可するか
    /// （trueを返すと無効化を許可する。通常はrequires_mfaがtrueなら拒否＝false）
    fn can_disable_last_factor(&self, roles: &[String]) -> bool {
        !self.requires_mfa(roles)
    }
}

/// 現在有効なMFA要素数を踏まえて、要素の無効化を許可してよいか判定する。
/// `remaining_factor_count`は対象要素を無効化した「後」に残る要素数。
pub fn can_disable_factor(
    policy: &dyn MfaPolicy,
    roles: &[String],
    remaining_factor_count: usize,
) -> bool {
    if remaining_factor_count > 0 {
        return true;
    }
    policy.can_disable_last_factor(roles)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AdminEmployeeRequiresMfa;
    impl MfaPolicy for AdminEmployeeRequiresMfa {
        fn requires_mfa(&self, roles: &[String]) -> bool {
            roles.iter().any(|r| r == "ADMIN" || r == "EMPLOYEE")
        }
    }

    #[test]
    fn last_factor_is_protected_for_required_roles() {
        let policy = AdminEmployeeRequiresMfa;
        let roles = vec!["ADMIN".to_string()];
        assert!(!can_disable_factor(&policy, &roles, 0));
        assert!(can_disable_factor(&policy, &roles, 1));
    }

    #[test]
    fn last_factor_is_removable_for_non_required_roles() {
        let policy = AdminEmployeeRequiresMfa;
        let roles = vec!["PARTNER".to_string()];
        assert!(can_disable_factor(&policy, &roles, 0));
    }
}
