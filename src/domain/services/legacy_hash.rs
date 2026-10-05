/// domain/services/legacy_hash.rs — bcrypt→Argon2 移行用レガシーハッシュ検証器
///
/// auth_core::domain::password::LegacyHashVerifier を実装し、Sophia既存の
/// bcryptハッシュ（$2a$/$2b$/$2y$）を検証する。verify_and_needs_rehashから
/// 呼ばれ、検証成功時はArgon2への再ハッシュが必要と判定される。
use auth_core::domain::password::LegacyHashVerifier;
use auth_core::error::AuthError;

/// bcrypt ハッシュ検証器
///
/// Sophia 既存のレガシー bcrypt ハッシュ（$2a$、$2b$、$2y$ プレフィックス）に対応し、
/// 検証成功時は Argon2 への再ハッシュが必要と判定される。
pub struct BcryptVerifier;

impl LegacyHashVerifier for BcryptVerifier {
    fn matches(&self, stored_hash: &str) -> bool {
        stored_hash.starts_with("$2a$")
            || stored_hash.starts_with("$2b$")
            || stored_hash.starts_with("$2y$")
    }

    fn verify(&self, password: &str, stored_hash: &str) -> Result<bool, AuthError> {
        bcrypt::verify(password, stored_hash).map_err(|e| AuthError::HashError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_2a_hash() {
        let verifier = BcryptVerifier;
        assert!(verifier.matches("$2a$12$abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn test_matches_2b_hash() {
        let verifier = BcryptVerifier;
        assert!(verifier.matches("$2b$12$abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn test_matches_2y_hash() {
        let verifier = BcryptVerifier;
        assert!(verifier.matches("$2y$12$abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn test_matches_false_for_argon2() {
        let verifier = BcryptVerifier;
        assert!(!verifier.matches("$argon2id$v=19$m=19456,t=2,p=1$xyz$abc"));
    }

    #[test]
    fn test_verify_correct_password() {
        let verifier = BcryptVerifier;
        let password = "test_password_123";
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();

        let result = verifier.verify(password, &hash);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[test]
    fn test_verify_wrong_password() {
        let verifier = BcryptVerifier;
        let password = "test_password_123";
        let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();

        let result = verifier.verify("wrong_password", &hash);
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
