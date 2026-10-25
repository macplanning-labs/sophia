//! 口座番号の値オブジェクト。DEMO-000233 項目4 / S2-G2-1
//!
//! - `AccountNumber`: 平文の口座番号(画面・帳票へ渡す側)。Debug は `****`。平文を取り出すのは `expose()` だけ。
//!   画面の JSON 用に `Serialize` を持つ(平文が出るのは、ここを通る応答だけ)。
//! - `StoredAccountNumber`: DB の中身(平文または `enc1:` の暗号文)。`Display` と `Serialize` を**持たない**。
//!   そのため、暗号化を通さずに帳票・JSON へ渡すコードはコンパイルできない。平文に戻せるのは
//!   `infrastructure::repositories::bank_account_repo` の復号関数だけ(生の値を取り出す口は同モジュールに限る)。
//!
//! 入力検証は `AccountNumber::parse_input`(S2-G2-4)に集約する。画面・マスタ・書き込みの全経路が通る。
//! 規則: 全角数字は半角に直す、ハイフン・空白は除く、残りは数字のみ、1〜20 桁(空は未登録として許す)、
//! `enc1:` で始まる入力は拒否。エラーは固定の日本語文言で、入力の値は含めない。

use serde::Serialize;

/// 入力の口座番号の桁数の上限(数字のみ)。既存列 VARCHAR(20) の上限に合わせる(計画書 5 章の 8 桁から緩めた。理由は計画書 16 章)
pub const INPUT_MAX_DIGITS: usize = 20;

/// 入力検証の失敗の理由(値は含まない)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountInputError {
    /// `enc1:` で始まる(暗号文と取り違えるため)
    Encrypted,
    /// 数字以外の文字がある(制御文字・英字・絵文字を含む)
    NotDigits,
    /// 数字が上限を超える
    TooLong,
}

impl AccountInputError {
    /// 利用者へ返す日本語の文言(値は含めない)
    pub fn message(self) -> &'static str {
        match self {
            Self::Encrypted => "口座番号の形式が正しくありません",
            Self::NotDigits => "口座番号は数字で入力してください",
            Self::TooLong => "口座番号は数字 1〜20 桁で入力してください",
        }
    }
}

impl std::fmt::Display for AccountInputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// 暗号化された値の接頭辞(`infrastructure::data_crypto::PREFIX` と同じ値。domain から infrastructure を参照しないため重複させる)
const ENCRYPTED_PREFIX: &str = "enc1:";

/// Debug・ログに出すときの代わり
const MASK: &str = "****";

/// 平文の口座番号。空は「未登録」を表す(失敗ではない)。
#[derive(Clone, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct AccountNumber(String);

impl AccountNumber {
    /// 平文として作る。`enc1:` で始まる値は、暗号文と取り違えるため拒否する
    pub fn new(value: &str) -> Result<Self, String> {
        if value.starts_with(ENCRYPTED_PREFIX) {
            return Err("口座番号に暗号化の接頭辞(enc1:)は使えません".into());
        }
        Ok(Self(value.to_string()))
    }

    /// 利用者の入力を検査して、保存できる形にする(S2-G2-4)。書き込みの全経路がこれを通る。
    ///
    /// - `enc1:` で始まる入力は拒否(暗号文と取り違えるため)
    /// - 全角数字は半角に直す。ハイフン類(`-` `－` `‐`〜`–` `−` `ー` `―` と幅ゼロの空白 U+200B)・空白(半角・全角・NBSP)は除く
    /// - 残りは数字のみ。1〜20 桁。空(除いた後も空)は未登録として許す
    /// - 制御文字・英字・絵文字・その他の記号は拒否
    pub fn parse_input(input: &str) -> Result<Self, AccountInputError> {
        if input.trim().starts_with(ENCRYPTED_PREFIX) {
            return Err(AccountInputError::Encrypted);
        }
        let mut digits = String::new();
        let mut has_visible = false;
        for ch in input.chars() {
            match ch {
                '0'..='9' => digits.push(ch),
                // 全角数字(０〜９)は半角に直す
                '\u{FF10}'..='\u{FF19}' => {
                    let offset = ch as u32 - 0xFF10;
                    digits.push(char::from_digit(offset, 10).unwrap_or('0'));
                }
                // ハイフン類は除く。日本語 IME が全角のハイフンとして出す「ー」(U+30FC)・「―」(U+2015)と、
                // 見えない幅ゼロの空白(U+200B)も同じ扱い(レビュー指摘 3)
                '-' | '\u{FF0D}' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2212}' | '\u{30FC}' | '\u{2015}' | '\u{200B}' => {
                    has_visible = true;
                }
                // 空白(半角・全角・NBSP)は除く
                ' ' | '\u{3000}' | '\u{00A0}' => {}
                _ => return Err(AccountInputError::NotDigits),
            }
            if digits.len() > INPUT_MAX_DIGITS {
                return Err(AccountInputError::TooLong);
            }
        }
        // ハイフンだけの入力は、数字が無いのに未登録になってしまうため拒否する
        if digits.is_empty() && has_visible {
            return Err(AccountInputError::NotDigits);
        }
        Ok(Self(digits))
    }

    /// 復号した値から作る(infrastructure の復号関数だけが使う)。`enc1:` の検査は、復号の前に済んでいる
    pub(crate) fn from_decrypted(value: String) -> Self {
        Self(value)
    }

    /// 平文を取り出す。帳票・画面へ渡すときだけ使う
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// 未登録(空)か
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for AccountNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(MASK)
    }
}

/// DB に入っている口座番号の値(平文または `enc1:` の暗号文)。平文として使うには復号が必要。
#[derive(Clone, PartialEq, Eq)]
pub struct StoredAccountNumber(String);

impl StoredAccountNumber {
    /// DB から読んだ値を包む(リポジトリの読み取りだけが使う)
    pub(crate) fn from_db(value: String) -> Self {
        Self(value)
    }

    /// DB の中身の生の値。復号関数(bank_account_repo の中だけ。tests/account_number_guard.rs で使用箇所を縛る)が使う
    pub(crate) fn raw(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for StoredAccountNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(MASK)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let a = AccountNumber::new("1234567").expect("平文として作れる");
        assert_eq!(format!("{a:?}"), "****");
        assert!(!format!("{a:?}").contains("1234567"));
        let s = StoredAccountNumber::from_db("enc1:k1:AAAA".into());
        assert_eq!(format!("{s:?}"), "****");
        assert!(!format!("{s:?}").contains("enc1"));
    }

    #[test]
    fn expose_returns_plain_value() {
        let a = AccountNumber::new("1234567").expect("平文として作れる");
        assert_eq!(a.expose(), "1234567");
        assert!(!a.is_empty());
        assert!(AccountNumber::new("").expect("空は未登録として作れる").is_empty());
    }

    #[test]
    fn rejects_encrypted_prefix_as_plain() {
        assert!(AccountNumber::new("enc1:k1:AAAA").is_err());
        assert!(AccountNumber::new("enc1:").is_err());
        // 接頭辞でなければ通す(大文字は別の値)
        assert!(AccountNumber::new("ENC1:x").is_ok());
    }

    fn parsed(input: &str) -> String {
        AccountNumber::parse_input(input).expect("受け付ける入力").expose().to_string()
    }

    #[test]
    fn parse_input_accepts_digits_and_normalizes_separators() {
        assert_eq!(parsed("1234567"), "1234567", "7 桁の普通預金");
        assert_eq!(parsed("0012345"), "0012345", "先頭の 0 は残す");
        assert_eq!(parsed("１２３４５６７"), "1234567", "全角数字は半角に");
        assert_eq!(parsed("123-4567"), "1234567", "ハイフンは除く");
        assert_eq!(parsed("１２３－４５６７"), "1234567", "全角のハイフンも除く");
        assert_eq!(parsed(" 123 4567 "), "1234567", "前後と途中の空白は除く");
        assert_eq!(parsed("１２３　４５６７"), "1234567", "全角の空白も除く");
        assert_eq!(parsed("12345678901234567890"), "12345678901234567890", "20 桁まで");
    }

    #[test]
    fn parse_input_treats_ime_dashes_and_zero_width_space_as_hyphen() {
        assert_eq!(parsed("123ー4567"), "1234567", "全角の長音記号(IME のハイフン)は除く");
        assert_eq!(parsed("123―4567"), "1234567", "横線(U+2015)は除く");
        assert_eq!(parsed("123\u{200B}4567"), "1234567", "ゼロ幅の空白は除く");
        // 数字が 1 つも無いハイフン類だけの入力は、従来どおり拒否する
        for bad in ["ー", "―", "\u{200B}", "ー－―"] {
            assert_eq!(AccountNumber::parse_input(bad).err(), Some(AccountInputError::NotDigits), "数字が無い: {bad:?}");
        }
    }

    #[test]
    fn parse_input_allows_empty_as_unregistered() {
        assert!(AccountNumber::parse_input("").expect("空は未登録").is_empty());
        assert!(AccountNumber::parse_input("   ").expect("空白だけは未登録").is_empty());
    }

    #[test]
    fn parse_input_rejects_non_digits_and_control_and_emoji() {
        for bad in ["12a4567", "ABC1234", "1234五", "12.34", "12/34", "１２３ａ", "1\u{0007}23", "1\u{0000}", "1\t2", "12😀", "12\n34", "-", " - "] {
            assert_eq!(
                AccountNumber::parse_input(bad).err(),
                Some(AccountInputError::NotDigits),
                "数字以外は拒否: {bad:?}"
            );
        }
    }

    #[test]
    fn parse_input_rejects_too_long_and_extremely_long() {
        assert_eq!(
            AccountNumber::parse_input("123456789012345678901").err(),
            Some(AccountInputError::TooLong),
            "21 桁は拒否"
        );
        let huge = "9".repeat(1_000_000);
        assert_eq!(AccountNumber::parse_input(&huge).err(), Some(AccountInputError::TooLong));
        let mixed = format!("{}x", "9".repeat(50));
        assert!(AccountNumber::parse_input(&mixed).is_err(), "極端に長い不正入力も拒否");
    }

    #[test]
    fn parse_input_rejects_encrypted_prefix() {
        assert_eq!(AccountNumber::parse_input("enc1:k1:AAAA").err(), Some(AccountInputError::Encrypted));
        assert_eq!(AccountNumber::parse_input("  enc1:k1:AAAA").err(), Some(AccountInputError::Encrypted));
        assert_eq!(AccountNumber::parse_input("enc1:").err(), Some(AccountInputError::Encrypted));
    }

    #[test]
    fn parse_input_messages_never_contain_the_value() {
        let input = "秘密12ab";
        let err = AccountNumber::parse_input(input).expect_err("拒否");
        assert!(!err.message().contains("秘密"));
        assert!(!err.message().contains("12ab"));
        assert_eq!(err.message(), "口座番号は数字で入力してください");
        assert_eq!(AccountInputError::TooLong.message(), "口座番号は数字 1〜20 桁で入力してください");
    }

    #[test]
    fn serialize_is_plain_for_screen_json_only() {
        let a = AccountNumber::new("1234567").expect("平文として作れる");
        assert_eq!(serde_json::to_string(&a).expect("JSON にできる"), "\"1234567\"");
    }
}
