//! domain/password.rs — パスワードハッシュ（Argon2）と旧方式からの移行
//!
//! WIPの旧実装（Djangoが稼働していた期間）は、Djangoの`Argon2PasswordHasher.encode()`
//! が使う`"argon2" + <PHC文字列>`という独自フォーマットでDBに保存する必要があった
//! （argon2クレートの`hash.to_string()`はPHC文字列のみを返すため、そのまま保存すると
//! Django側がハッシュ方式を特定できずログイン不能になっていた）。Django(web)は
//! 2026-08-10に完全撤去済みのため、このプレフィックスはもはや不要（`hash_password`は
//! 素のPHC文字列のみを返す。既存の"argon2"プレフィックス付きレコードは`verify_argon2`
//! が引き続き読み取れる）。
//!
//! 方針ドキュメント3.2節はこれを一般化した `verify_and_needs_rehash` を
//! 提案している。ここでは「旧ハッシュ方式の検証器」を `LegacyHashVerifier`
//! トレイトとして差し替え可能にし、Django PBKDF2検証器を用意している。
//! bcrypt等の他方式が必要な場合は同じトレイトを実装して追加する。

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

use crate::error::AuthError;

/// 素のArgon2 PHC文字列（`$argon2id$...`）を返す。
pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AuthError::HashError(e.to_string()))
}

/// Argon2ハッシュ（`$argon2id$...`、または`"argon2"`プレフィックス付き）を検証する。
pub fn verify_argon2(password: &str, stored_hash: &str) -> Result<bool, AuthError> {
    // Django形式("argon2"+PHC文字列)なら先頭の"argon2"を取り除いてから解釈する。
    let phc_str = stored_hash.strip_prefix("argon2").unwrap_or(stored_hash);
    let parsed_hash =
        PasswordHash::new(phc_str).map_err(|e| AuthError::HashError(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

/// 旧ハッシュ方式（Django PBKDF2、bcrypt等）を判定・検証するトレイト。
/// 「自分が対象とするハッシュか」を`matches`で判定し、対象であれば`verify`で
/// 実際の検証を行う。`password::verify_and_needs_rehash`はこの実装を差し替えて
/// アプリごとの移行元ハッシュに対応する。
pub trait LegacyHashVerifier: Send + Sync {
    /// このverifierが対象とするハッシュ文字列かどうか
    fn matches(&self, stored_hash: &str) -> bool;
    fn verify(&self, password: &str, stored_hash: &str) -> Result<bool, AuthError>;
}

/// WIP用: Django PBKDF2-SHA256ハッシュ（`pbkdf2_sha256$<iterations>$<salt>$<hash>`）の検証器。
pub struct DjangoPbkdf2Verifier;

impl LegacyHashVerifier for DjangoPbkdf2Verifier {
    fn matches(&self, stored_hash: &str) -> bool {
        stored_hash.starts_with("pbkdf2_sha256$")
    }

    fn verify(&self, password: &str, stored_hash: &str) -> Result<bool, AuthError> {
        use base64::Engine;
        use hmac::Hmac;
        use sha2::Sha256;

        let parts: Vec<&str> = stored_hash.split('$').collect();
        if parts.len() != 4 || parts[0] != "pbkdf2_sha256" {
            return Ok(false);
        }

        let iterations: u32 = parts[1]
            .parse()
            .map_err(|_| AuthError::HashError("PBKDF2 iterations parse error".to_string()))?;
        let salt = parts[2];
        let expected_hash = parts[3];

        let mut derived_key = vec![0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha256>>(
            password.as_bytes(),
            salt.as_bytes(),
            iterations,
            &mut derived_key,
        )
        .map_err(|e| AuthError::HashError(format!("PBKDF2 error: {e}")))?;

        let computed_hash = base64::engine::general_purpose::STANDARD.encode(&derived_key);
        Ok(computed_hash == expected_hash)
    }
}

/// パスワードを検証し、旧ハッシュ方式であれば「Argon2への再ハッシュが必要」
/// という情報を合わせて返す（方針ドキュメント3.2節）。
///
/// 再ハッシュ後のDB更新はauth-coreの責務外（アプリのユーザーリポジトリが行う）。
/// 呼び出し側は戻り値の`Some(new_hash)`を見てDBを更新すること。
pub fn verify_and_needs_rehash(
    password: &str,
    stored_hash: &str,
    legacy_verifiers: &[&dyn LegacyHashVerifier],
) -> Result<(bool, Option<String>), AuthError> {
    for verifier in legacy_verifiers {
        if verifier.matches(stored_hash) {
            let valid = verifier.verify(password, stored_hash)?;
            if !valid {
                return Ok((false, None));
            }
            let new_hash = hash_password(password)?;
            return Ok((true, Some(new_hash)));
        }
    }

    // 旧方式に該当しなければArgon2として検証する
    let valid = verify_argon2(password, stored_hash)?;
    Ok((valid, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_roundtrip() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_argon2("correct horse battery staple", &hash).unwrap());
        assert!(!verify_argon2("wrong password", &hash).unwrap());
    }

    #[test]
    fn legacy_django_prefixed_hash_still_verifies() {
        // Django運用時代に書き込まれた"argon2"プレフィックス付きレコードが、
        // プレフィックスを付けなくなった現行のhash_passwordとは別に、
        // 引き続き検証できることを確認する（後方互換性の担保）。
        let plain_hash = hash_password("correct horse battery staple").unwrap();
        let legacy_style_hash = format!("argon2{plain_hash}");
        assert!(verify_argon2("correct horse battery staple", &legacy_style_hash).unwrap());
    }

    #[test]
    fn verify_and_needs_rehash_promotes_pbkdf2() {
        // pbkdf2_sha256$<iterations>$<salt>$<base64hash> を手計算で作る代わりに、
        // 実装と同じ手順でテスト用ハッシュを作る。
        use base64::Engine;
        use hmac::Hmac;
        use sha2::Sha256;

        let password = "correct horse battery staple";
        let salt = "testsalt";
        let iterations = 1000u32;
        let mut derived_key = vec![0u8; 32];
        pbkdf2::pbkdf2::<Hmac<Sha256>>(
            password.as_bytes(),
            salt.as_bytes(),
            iterations,
            &mut derived_key,
        )
        .unwrap();
        let hash_b64 = base64::engine::general_purpose::STANDARD.encode(&derived_key);
        let stored = format!("pbkdf2_sha256${iterations}${salt}${hash_b64}");

        let verifiers: Vec<&dyn LegacyHashVerifier> = vec![&DjangoPbkdf2Verifier];
        let (valid, new_hash) = verify_and_needs_rehash(password, &stored, &verifiers).unwrap();
        assert!(valid);
        let new_hash = new_hash.expect("再ハッシュが必要と判定されるはず");
        assert!(new_hash.starts_with("$argon2id$"));
        assert!(verify_argon2(password, &new_hash).unwrap());
    }

    #[test]
    fn verify_and_needs_rehash_no_migration_for_argon2() {
        let hash = hash_password("correct horse battery staple").unwrap();
        let verifiers: Vec<&dyn LegacyHashVerifier> = vec![&DjangoPbkdf2Verifier];
        let (valid, new_hash) =
            verify_and_needs_rehash("correct horse battery staple", &hash, &verifiers).unwrap();
        assert!(valid);
        assert!(new_hash.is_none());
    }
}
