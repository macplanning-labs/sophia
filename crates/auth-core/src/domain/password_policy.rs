//! domain/password_policy.rs — パスワード強度バリデータ（方針ドキュメント5章）
//!
//! バリデータ本体はauth-core、パラメータ（最小長・文字種要件）は各アプリが注入する。
//! 利用側アプリごとに異なるポリシーを維持したい場合にも対応できる。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PasswordPolicy {
    pub min_length: usize,
    pub require_upper: bool,
    pub require_lower: bool,
    pub require_digit: bool,
    pub require_symbol: bool,
}

impl PasswordPolicy {
    /// 強めの既定ポリシー（12文字以上＋英大文字・小文字・数字必須）
    pub fn strict_default() -> Self {
        Self {
            min_length: 12,
            require_upper: true,
            require_lower: true,
            require_digit: true,
            require_symbol: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PasswordPolicyViolation {
    TooShort { min_length: usize },
    MissingUpper,
    MissingLower,
    MissingDigit,
    MissingSymbol,
}

impl PasswordPolicy {
    /// パスワードを検証する。違反があれば全て列挙して返す
    /// （最初の1件で打ち切らず、フロントエンドがまとめてUI表示できるようにする）。
    pub fn validate(&self, password: &str) -> Result<(), Vec<PasswordPolicyViolation>> {
        let mut violations = Vec::new();

        if password.chars().count() < self.min_length {
            violations.push(PasswordPolicyViolation::TooShort {
                min_length: self.min_length,
            });
        }
        if self.require_upper && !password.chars().any(|c| c.is_ascii_uppercase()) {
            violations.push(PasswordPolicyViolation::MissingUpper);
        }
        if self.require_lower && !password.chars().any(|c| c.is_ascii_lowercase()) {
            violations.push(PasswordPolicyViolation::MissingLower);
        }
        if self.require_digit && !password.chars().any(|c| c.is_ascii_digit()) {
            violations.push(PasswordPolicyViolation::MissingDigit);
        }
        if self.require_symbol && !password.chars().any(|c| !c.is_ascii_alphanumeric()) {
            violations.push(PasswordPolicyViolation::MissingSymbol);
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_policy_accepts_valid_password() {
        let policy = PasswordPolicy::strict_default();
        assert!(policy.validate("CorrectHorse9Battery").is_ok());
    }

    #[test]
    fn strict_policy_rejects_short_password() {
        let policy = PasswordPolicy::strict_default();
        let violations = policy.validate("Ab1").unwrap_err();
        assert!(violations.contains(&PasswordPolicyViolation::TooShort { min_length: 12 }));
    }

    #[test]
    fn strict_policy_reports_all_violations() {
        let policy = PasswordPolicy::strict_default();
        let violations = policy.validate("alllowercase").unwrap_err();
        assert!(violations.contains(&PasswordPolicyViolation::MissingUpper));
        assert!(violations.contains(&PasswordPolicyViolation::MissingDigit));
    }
}
