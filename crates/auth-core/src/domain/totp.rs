//! domain/totp.rs — TOTP（ワンタイムパスワード）
//!
//! WIPの `totp_service.rs`（秘密鍵のAES-GCM暗号化保存・QR生成）と
//! `auth_service.rs` のTOTP部分（Django/pyotp互換のBase32平文シークレットに
//! 対する検証。WIPは現状こちらを本線として使っている）を統合した。
//!
//! ## 保存方式の方針（2026-08-14決定）
//! TOTP秘密鍵の保存方式は **AES-GCM暗号化保存に統一する**（方針ドキュメント1.5節）。
//! - 新規のTOTP enrollment（新規実装・再登録）には [`encrypt_secret`] /
//!   [`decrypt_secret`] を使うこと。これが正式な標準。
//! - Djangoの `pyotp` と共有しているテーブル（Base32平文で保存）に対する
//!   [`verify_code_base32`] は、Django稼働中の後方互換のためだけに残している
//!   （1.4節のDjango段階的廃止方針を参照）。Django廃止のタイミングで、既存の
//!   平文シークレットをAES-GCM暗号化ブロブへ移行するバッチ処理（または
//!   `domain::password::verify_and_needs_rehash`と同様のon-the-fly移行）を
//!   別途実装すること。

use aes_gcm::aead::generic_array::GenericArray;
use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::error::AuthError;

/// AES-256-GCM 暗号化結果。DB保存時のサイズ制限（64文字）のため、
/// ciphertext + nonce を一つのblobとして保存する。
pub struct EncryptedSecret {
    pub blob_b64: String,
}

/// 暗号鍵をアプリのSECRET_KEYから派生する（SHA-256）。
fn derive_key(secret_key: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret_key.as_bytes());
    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

// ── 秘密鍵の暗号化/復号 ──

/// TOTP秘密鍵をAES-256-GCMで暗号化する。
/// 戻り値: Base64(ciphertext + nonceを連結したblob)。
/// サイズ: 36 bytes ciphertext + 12 bytes nonce = 48 bytes → 64 chars base64。
pub fn encrypt_secret(plaintext: &[u8], secret_key: &str) -> Result<EncryptedSecret, AuthError> {
    let key_bytes = derive_key(secret_key);
    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| AuthError::Totp(format!("秘密鍵の暗号化に失敗: {e}")))?;

    let mut blob = Vec::with_capacity(ciphertext.len() + nonce_bytes.len());
    blob.extend_from_slice(&ciphertext);
    blob.extend_from_slice(&nonce_bytes);

    Ok(EncryptedSecret {
        blob_b64: BASE64.encode(&blob),
    })
}

/// AES-256-GCMで暗号化されたTOTP秘密鍵を復号する。
pub fn decrypt_secret(blob_b64: &str, secret_key: &str) -> Result<Vec<u8>, AuthError> {
    let key_bytes = derive_key(secret_key);
    let key = GenericArray::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let blob = BASE64
        .decode(blob_b64)
        .map_err(|e| AuthError::Totp(format!("秘密鍵blobのデコードに失敗: {e}")))?;
    if blob.len() < 12 {
        return Err(AuthError::Totp("秘密鍵blobが短すぎます".to_string()));
    }

    let ciphertext = &blob[..blob.len() - 12];
    let nonce_bytes = &blob[blob.len() - 12..];
    let nonce = Nonce::from_slice(nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| AuthError::Totp(format!("秘密鍵の復号に失敗: {e}")))
}

// ── TOTP 操作 ──

/// 新しいTOTP秘密鍵を生成する（バイト列、20バイト）。
pub fn generate_secret() -> Vec<u8> {
    use rand::RngCore;
    let mut secret_bytes = vec![0u8; 20];
    rand::thread_rng().fill_bytes(&mut secret_bytes);
    secret_bytes
}

fn build_totp(secret_bytes: &[u8], issuer: &str, username: &str) -> Result<TOTP, AuthError> {
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes.to_vec(),
        Some(issuer.to_string()),
        username.to_string(),
    )
    .map_err(|e| AuthError::Totp(format!("TOTPの構築に失敗: {e}")))
}

/// QRコードをbase64エンコードされたPNGとして返す。
/// フロントエンドで `<img src="data:image/png;base64,{qr_base64}">` として表示する。
pub fn generate_qr_base64(
    secret_bytes: &[u8],
    issuer: &str,
    username: &str,
) -> Result<String, AuthError> {
    let totp = build_totp(secret_bytes, issuer, username)?;
    totp.get_qr_base64()
        .map_err(|e| AuthError::Totp(format!("QRコードの生成に失敗: {e}")))
}

/// TOTPコードを検証する（バイト列シークレット版）。現在時刻の前後±30秒（skew=1）を許容する。
pub fn verify_code(
    secret_bytes: &[u8],
    issuer: &str,
    username: &str,
    code: &str,
) -> Result<bool, AuthError> {
    let totp = build_totp(secret_bytes, issuer, username)?;
    Ok(totp.check_current(code).unwrap_or(false))
}

/// 秘密鍵のBase32表現を返す（手動入力用のバックアップコード表示）。
pub fn secret_to_base32(secret_bytes: &[u8]) -> String {
    Secret::Raw(secret_bytes.to_vec()).to_encoded().to_string()
}

/// TOTPコードを検証する（Django/pyotp互換のBase32平文シークレット版）。
/// DB保存されているsecretは`pyotp.random_base32()`が生成するBase32エンコード済み
/// 文字列であり、`Secret::Raw`だと文字列のバイト列をそのまま秘密鍵として扱って
/// しまいBase32デコードされず、pyotp側と異なる鍵で検証することになり必ず失敗する
/// ため、`Secret::Encoded`で正しくBase32デコードする。
pub fn verify_code_base32(
    secret_base32: &str,
    issuer: &str,
    code: &str,
) -> Result<bool, AuthError> {
    let secret_bytes = Secret::Encoded(secret_base32.to_string())
        .to_bytes()
        .map_err(|e| AuthError::Totp(format!("TOTP secret error: {e}")))?;

    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some(issuer.to_string()),
        String::new(),
    )
    .map_err(|e| AuthError::Totp(format!("TOTP error: {e}")))?;

    Ok(totp.check_current(code).unwrap_or(false))
}

/// TOTPセットアップ用に新規シークレット（Base32）とprovisioning URIを生成する。
pub fn generate_totp_setup(issuer: &str, username: &str) -> Result<(String, String), AuthError> {
    let secret_bytes = generate_secret();
    let secret_base32 = secret_to_base32(&secret_bytes);
    let totp = build_totp(&secret_bytes, issuer, username)?;
    Ok((secret_base32, totp.get_url()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let secret_key = "test-secret-key-for-wip";
        let original = b"test-totp-secret-bytes-here";

        let encrypted = encrypt_secret(original, secret_key).unwrap();
        let decrypted = decrypt_secret(&encrypted.blob_b64, secret_key).unwrap();

        assert_eq!(original.to_vec(), decrypted);
    }

    #[test]
    fn test_blob_size_fits_in_db() {
        let secret_key = "test-secret-key";
        let secret = generate_secret();
        let encrypted = encrypt_secret(&secret, secret_key).unwrap();
        assert!(encrypted.blob_b64.len() <= 64);
    }

    #[test]
    fn test_generate_secret_is_not_empty() {
        assert!(!generate_secret().is_empty());
    }

    #[test]
    fn base32_roundtrip_matches_current_code() {
        let (secret_base32, _uri) = generate_totp_setup("WIP", "alice").unwrap();
        let secret_bytes = Secret::Encoded(secret_base32.clone()).to_bytes().unwrap();
        let totp = build_totp(&secret_bytes, "WIP", "alice").unwrap();
        let code = totp.generate_current().unwrap();
        assert!(verify_code_base32(&secret_base32, "WIP", &code).unwrap());
    }
}
