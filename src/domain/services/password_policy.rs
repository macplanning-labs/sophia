//! Shared password policy for staff user create / reset (P5-1e).

pub const MIN_LEN: usize = 12;

pub fn validate(password: &str) -> Result<(), String> {
    if password.chars().count() < MIN_LEN {
        return Err(format!("パスワードは{MIN_LEN}文字以上にしてください"));
    }
    let has_upper = password.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = password.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !(has_upper && has_lower && has_digit) {
        return Err(
            "パスワードには英大文字・英小文字・数字をそれぞれ1文字以上含めてください".to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_short() {
        assert!(validate("Abcd12345").is_err()); // 9 chars
    }

    #[test]
    fn rejects_no_digit() {
        assert!(validate("Abcdefghijkl").is_err());
    }

    #[test]
    fn accepts_strong() {
        assert!(validate("Abcdefghij12").is_ok());
    }
}
