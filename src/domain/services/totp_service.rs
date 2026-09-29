//! domain/services/totp_service.rs — TOTP ワンタイムパスワードサービス（auth-coreの薄いラッパー）
//!
//! 秘密鍵生成・QR生成・コード検証などの純粋ロジックは auth_core::domain::totp に
//! 集約されている。ここでは Sophia 固有の issuer 名("Sophia")の付与と、DB保存形式
//! （secret_encrypted/nonce の別カラム、s_totp_device）への適合のみを行う。

use auth_core::domain::totp as core_totp;
use auth_core::error::AuthError;

pub use core_totp::EncryptedSecretSplit as EncryptedSecret;

const ISSUER: &str = "Sophia";

/// 新しい TOTP 秘密鍵を生成する（Base32 エンコード済み）
pub fn generate_secret() -> Vec<u8> {
    core_totp::generate_secret()
}

/// QR コードを base64 エンコードされた PNG として返す
/// フロントエンドで `<img src="data:image/png;base64,{qr_base64}">` で表示
pub fn generate_qr_base64(secret_bytes: &[u8], email: &str) -> Result<String, AuthError> {
    core_totp::generate_qr_base64(secret_bytes, ISSUER, email)
}

/// TOTP コードを検証する
/// 現在時刻の前後 ±30秒（skew=1）のコードも許容
pub fn verify_code(secret_bytes: &[u8], email: &str, code: &str) -> Result<bool, AuthError> {
    core_totp::verify_code(secret_bytes, ISSUER, email, code)
}

/// 秘密鍵の Base32 表現を返す（手動入力用のバックアップコード表示）
pub fn secret_to_base32(secret_bytes: &[u8]) -> String {
    core_totp::secret_to_base32(secret_bytes)
}

/// TOTP 秘密鍵を AES-256-GCM で暗号化する（ciphertext/nonce 別カラム保存）
pub fn encrypt_secret(plaintext: &[u8], secret_key: &str) -> Result<EncryptedSecret, AuthError> {
    core_totp::encrypt_secret_split(plaintext, secret_key)
}

/// AES-256-GCM で暗号化された TOTP 秘密鍵を復号する
pub fn decrypt_secret(
    ciphertext_b64: &str,
    nonce_b64: &str,
    secret_key: &str,
) -> Result<Vec<u8>, AuthError> {
    core_totp::decrypt_secret_split(ciphertext_b64, nonce_b64, secret_key)
}

// ── テスト ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret_key = "test-secret-key-for-sophia";
        let original = b"test-totp-secret-bytes-here";

        let encrypted = encrypt_secret(original, secret_key).unwrap();
        let decrypted = decrypt_secret(&encrypted.ciphertext_b64, &encrypted.nonce_b64, secret_key).unwrap();

        assert_eq!(original.to_vec(), decrypted);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let original = b"test-totp-secret";
        let encrypted = encrypt_secret(original, "correct-key").unwrap();
        let result = decrypt_secret(&encrypted.ciphertext_b64, &encrypted.nonce_b64, "wrong-key");
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_secret_is_not_empty() {
        let secret = generate_secret();
        assert!(!secret.is_empty());
    }

    #[test]
    fn test_qr_code_generation() {
        let secret = generate_secret();
        let qr = generate_qr_base64(&secret, "test@example.com");
        assert!(qr.is_ok());
        assert!(!qr.unwrap().is_empty());
    }

    #[test]
    fn test_verify_current_code() {
        let secret = generate_secret();
        let email = "test@example.com";
        let totp = totp_rs::TOTP::new(
            totp_rs::Algorithm::SHA1,
            6,
            1,
            30,
            secret.clone(),
            Some(ISSUER.to_string()),
            email.to_string(),
        )
        .unwrap();
        let code = totp.generate_current().unwrap();
        let result = verify_code(&secret, email, &code).unwrap();
        assert!(result);
    }

    #[test]
    fn test_verify_wrong_code_fails() {
        let secret = generate_secret();
        let result = verify_code(&secret, "test@example.com", "000000").unwrap();
        let _ = result;
    }
}
