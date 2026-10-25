//! 列の暗号化(口座番号などの、データ単位で守る値)。DEMO-000233 項目4 / S2-G1(土台)
//!
//! - 形式: `enc1:<鍵ID>:<base64(nonce(12バイト) + 暗号文)>`。平文と同じ列に入れる。`enc1:` で始まるかどうかで判定する。
//! - 暗号: `integrations::secret::SecretCipher`(AES-256-GCM、鍵ID付き、鍵ローテーション対応)を使う。
//!   AAD に「表.列」(例 `m_partner.account_number`)を入れるので、別の列に入れ替えられたら復号に失敗する。
//! - 鍵: 環境変数 `DATA_KEYS_FILE` の場所(中身は SecretCipher と同じ JSON)。`SECRET_KEY` や外部連携の鍵とは別にする。
//!   鍵IDは `[A-Za-z0-9_-]` の 1〜32 文字(`:` を含むと形式が壊れるため)。
//! - 環境変数(いずれも既定は無効・平文のまま。値は true/false/1/0/yes/no。空は false、それ以外は起動エラー):
//!   - `DATA_KEYS_FILE`: 鍵ファイルの場所
//!   - `DATA_ENCRYPTION_ENABLED=true`: 書き込みを暗号化する(鍵が必須)
//!   - `DATA_ENCRYPTION_REQUIRED=true`: 平文の読み取りを拒否する(S2-G3 の切替。ENABLED も必要)
//! - 起動時の扱い: `local` / `dev` / `test` 以外(ENV_NAME 未設定を含む)では、ENABLED・REQUIRED・DATA_KEYS_FILE のどれかが
//!   有効なのに鍵が読めなければ起動を失敗させる。`local` / `dev` / `test` では警告して暗号化を無効にする。
//! - 鍵ファイルの権限が 600 でなければ、`local` / `dev` / `test` 以外では起動を失敗させ、それ以外では警告する。
//!
//! 鍵・平文は、ログにもエラーの文字列にも出さない。

use base64::Engine;

use crate::domain::services::static_regex::compile_static;
use crate::infrastructure::integrations::secret::SecretCipher;

/// 暗号化された値の接頭辞
pub const PREFIX: &str = "enc1:";

/// このビルドのアプリが、暗号化された値を読めるか(DEMO-000233 S2-G2)。
/// true: 口座番号の読み書きは全箇所で data_crypto を通る(S2-G2-5)。false のときは、変換コマンド(`encrypt-bank-accounts --execute`)を拒否する。
pub const APP_READS_ENCRYPTED: bool = true;

/// 鍵IDの最大の長さ
const MAX_KEY_ID_LEN: usize = 32;

/// 暗号化の設定と鍵(起動時に一度組み立てる)
pub struct DataCrypto {
    cipher: Option<SecretCipher>,
    enabled: bool,
    required: bool,
}

impl std::fmt::Debug for DataCrypto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataCrypto")
            .field("enabled", &self.enabled)
            .field("required", &self.required)
            .field("key_loaded", &self.cipher.is_some())
            .finish()
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// 真偽の値を読む。空・未設定は false。true/false/1/0/yes/no(大文字小文字は問わない)以外は Err
pub fn parse_flag(name: &str, value: Option<&str>) -> Result<bool, String> {
    let v = value.map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
    match v.as_str() {
        "" | "false" | "0" | "no" => Ok(false),
        "true" | "1" | "yes" => Ok(true),
        _ => Err(format!("{name} の値が不正です(true / false / 1 / 0 / yes / no のいずれかにしてください)")),
    }
}

/// 鍵IDの形式が正しいか(`[A-Za-z0-9_-]`、1〜32 文字)
pub fn valid_key_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_KEY_ID_LEN && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 開発用の環境か(ここ以外は、有効化したのに鍵が無ければ起動を止める)。serve の起動時の点検も同じ基準を使う
pub fn is_dev_env(env_name: &str) -> bool {
    matches!(env_name, "local" | "dev" | "test")
}

impl DataCrypto {
    /// 環境変数から組み立てる(起動時・変換コマンド用)。ENV_NAME 未設定は「開発以外」として扱う
    pub fn from_env() -> Result<Self, String> {
        let env_name = std::env::var("ENV_NAME").unwrap_or_default();
        let keys_file = env_nonempty("DATA_KEYS_FILE");
        let enabled = parse_flag("DATA_ENCRYPTION_ENABLED", std::env::var("DATA_ENCRYPTION_ENABLED").ok().as_deref())?;
        let required = parse_flag("DATA_ENCRYPTION_REQUIRED", std::env::var("DATA_ENCRYPTION_REQUIRED").ok().as_deref())?;
        Self::build(&env_name, keys_file.as_deref(), enabled, required)
    }

    /// 設定から組み立てる(環境変数を読まない)。起動時の失敗条件をここにまとめる
    pub fn build(env_name: &str, keys_file: Option<&str>, enabled: bool, required: bool) -> Result<Self, String> {
        if required && !enabled {
            return Err("DATA_ENCRYPTION_REQUIRED=true には DATA_ENCRYPTION_ENABLED=true が必要です".into());
        }
        let strict = !is_dev_env(env_name);
        let wanted = enabled || required || keys_file.is_some();

        let mut enabled = enabled;
        let cipher = match keys_file {
            None => None,
            Some(path) => match SecretCipher::from_file(path) {
                Ok(c) => {
                    check_key_ids(&c)?;
                    check_file_mode(path, strict)?;
                    Some(c)
                }
                Err(e) => {
                    if strict && wanted {
                        return Err(format!("データ暗号化の鍵を読めません(DATA_KEYS_FILE): {e}"));
                    }
                    if required {
                        return Err("DATA_ENCRYPTION_REQUIRED=true ですが、データ暗号化の鍵を読めません".into());
                    }
                    tracing::warn!("[データ暗号化] 鍵を読めないため、暗号化を無効にします(開発用): {e}");
                    None
                }
            },
        };

        if cipher.is_none() {
            if required {
                return Err("DATA_ENCRYPTION_REQUIRED=true ですが、DATA_KEYS_FILE が設定されていません".into());
            }
            if enabled {
                if strict {
                    return Err("DATA_ENCRYPTION_ENABLED=true ですが、DATA_KEYS_FILE が設定されていません".into());
                }
                tracing::warn!("[データ暗号化] DATA_KEYS_FILE が無いため、開発用に暗号化を無効にします");
                enabled = false;
            }
        }

        Ok(Self { cipher, enabled, required })
    }

    /// 書き込みを暗号化するか
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// 平文の読み取りを拒否するか
    pub fn required(&self) -> bool {
        self.required
    }

    /// 鍵(変換コマンドが使う)。鍵が読めていなければ None
    pub fn cipher(&self) -> Option<&SecretCipher> {
        self.cipher.as_ref()
    }

    /// 値が暗号化された形式か
    pub fn is_encrypted(stored: &str) -> bool {
        stored.starts_with(PREFIX)
    }

    /// 暗号化された値の鍵ID(平文なら None)
    pub fn key_id_of(stored: &str) -> Option<&str> {
        stored.strip_prefix(PREFIX).and_then(|rest| rest.split_once(':')).map(|(id, _)| id)
    }

    /// 書き込む値。暗号化が有効なら `enc1:` 形式、無効なら平文のまま(移行期間・開発)
    pub fn write_value(&self, column: &str, plain: &str) -> Result<String, String> {
        if !self.enabled {
            return Ok(plain.to_string());
        }
        self.encrypt(column, plain)
    }

    /// 読む値。`enc1:` は復号し、平文は REQUIRED でなければそのまま返す
    pub fn read_value(&self, column: &str, stored: &str) -> Result<String, String> {
        if Self::is_encrypted(stored) {
            return self.decrypt(column, stored);
        }
        if self.required {
            return Err(format!("{column} の値が平文のままです(DATA_ENCRYPTION_REQUIRED=true のため拒否)"));
        }
        Ok(stored.to_string())
    }

    /// 必ず暗号化する(有効・無効に関係なく。変換コマンドが使う)。鍵が無ければ Err
    pub fn encrypt(&self, column: &str, plain: &str) -> Result<String, String> {
        let c = self.cipher.as_ref().ok_or("暗号化の鍵が読めていないため、暗号化できません")?;
        let sealed = c.seal_with_aad(plain.as_bytes(), column.as_bytes())?;
        let mut blob = sealed.nonce.clone();
        blob.extend_from_slice(&sealed.ciphertext);
        Ok(format!("{PREFIX}{}:{}", sealed.key_id, base64::engine::general_purpose::STANDARD.encode(blob)))
    }

    /// `enc1:` 形式の値を復号する。形式が違う・鍵が無い・改ざん・列の入れ替えは Err
    pub fn decrypt(&self, column: &str, stored: &str) -> Result<String, String> {
        let c = self.cipher.as_ref().ok_or("暗号化された値を読むための鍵が読めていません")?;
        let rest = stored.strip_prefix(PREFIX).ok_or("暗号化の形式(enc1:)ではありません")?;
        let (key_id, b64) = rest.split_once(':').ok_or("暗号化の形式が正しくありません")?;
        if key_id.is_empty() {
            return Err("暗号化の形式が正しくありません(鍵ID がありません)".into());
        }
        let blob = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|_| "暗号化の形式が正しくありません(base64 ではありません)".to_string())?;
        if blob.len() <= 12 {
            return Err("暗号化の形式が正しくありません(長さが足りません)".into());
        }
        let (nonce, ciphertext) = blob.split_at(12);
        let plain = c.open_with_aad(key_id, nonce, ciphertext, column.as_bytes())
            .map_err(|e| format!("{column}: {e}"))?;
        String::from_utf8(plain).map_err(|_| format!("{column}: 復号した値が UTF-8 ではありません"))
    }
}

/// プロセスで 1 つの設定(起動時に `install` する。リポジトリ層が `global()` で使う)
static GLOBAL: std::sync::OnceLock<DataCrypto> = std::sync::OnceLock::new();

/// 起動時に 1 回だけ設定する。二度目は失敗する
pub fn install(dc: DataCrypto) -> Result<(), String> {
    GLOBAL.set(dc).map_err(|_| "データ暗号化の設定は既に初期化されています".to_string())
}

/// 設定を取り出す。未初期化なら Err(設定忘れで平文のまま動く事故を防ぐため、既定値を返さない)
pub fn global() -> Result<&'static DataCrypto, String> {
    GLOBAL.get().ok_or_else(|| "データ暗号化の設定が初期化されていません(起動時の install が必要です)".to_string())
}

/// 暗号文の形: `enc1:<鍵ID>:<base64>`。base64 は nonce 12 バイト + タグ 16 バイト以上(38 文字以上)。
/// 平文の中に `enc1:` という字が偶然あっても、この形に合わないものは検知しない(誤検知を避ける)。
const CIPHERTEXT_PATTERN: &str = r"enc1:[A-Za-z0-9_-]{1,32}:[A-Za-z0-9+/]{38,}={0,2}";

static CIPHERTEXT_RX: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| compile_static(CIPHERTEXT_PATTERN));

fn ciphertext_rx() -> &'static regex::Regex {
    &CIPHERTEXT_RX
}

/// 帳票・API 応答に渡す文字列(JSON など)に、暗号文の形の値が混ざっていないかを確かめる。
/// JSON の文字列値の中の暗号文の形を探す(base64 の字と `:` `_` `-` は JSON の中でエスケープされないので、
/// 文字列として探せば値の中だけを見たことになる。大きな JSON を木に組み直さないためにこの形にしている)。
/// 混ざっていれば Err。Err の文字列には値も箇所も含めない(呼び出し側が経路だけを記録する)。
pub fn ensure_no_encrypted_payload(payload: &str) -> Result<(), String> {
    if ciphertext_rx().is_match(payload) {
        return Err("出力に暗号化された値が混ざっているため、出力を止めました".into());
    }
    Ok(())
}

/// 鍵ファイルの中の全ての鍵IDの形式を確かめる(形式が違うと、暗号文の区切りが壊れる)
fn check_key_ids(c: &SecretCipher) -> Result<(), String> {
    if c.key_ids().iter().all(|id| valid_key_id(id)) {
        Ok(())
    } else {
        Err("データ暗号化の鍵IDの形式が不正です(英数字・_・-、1〜32 文字)".into())
    }
}

/// 鍵ファイルの権限が 600 か(中身は読まない・出さない)。開発以外では 600 でなければ起動を止める
#[cfg(unix)]
fn check_file_mode(path: &str, strict: bool) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    let mode = meta.permissions().mode() & 0o777;
    if mode == 0o600 {
        return Ok(());
    }
    if strict {
        return Err(format!("データ暗号化の鍵ファイルの権限が {mode:o} です。本番・ステージングでは 600 にしてください"));
    }
    tracing::warn!("[データ暗号化] 鍵ファイルの権限が {:o} です。600 にしてください", mode);
    Ok(())
}

#[cfg(not(unix))]
fn check_file_mode(_path: &str, _strict: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::integrations::secret::new_key_b64;

    const COL: &str = "m_partner.account_number";

    fn keys_json(current: &str, ids_and_keys: &[(&str, &str)]) -> String {
        let keys: Vec<String> = ids_and_keys.iter().map(|(id, k)| format!("\"{id}\":\"{k}\"")).collect();
        format!("{{\"current\":\"{current}\",\"keys\":{{{}}}}}", keys.join(","))
    }

    fn cipher_from(json: &str) -> SecretCipher {
        SecretCipher::from_json(json).expect("鍵を読めること")
    }

    /// テスト用に、鍵ファイル(一時ファイル)を作る。鍵はテストの中で生成する
    fn key_file(tag: &str, json: &str) -> String {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("時刻").as_nanos();
        let path = std::env::temp_dir().join(format!("sophia-data-keys-{tag}-{nanos}.json"));
        std::fs::write(&path, json).expect("一時ファイルを書けること");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).expect("権限を 600 にできること");
        }
        path.to_string_lossy().into_owned()
    }

    fn dc_with(cipher: SecretCipher, enabled: bool, required: bool) -> DataCrypto {
        DataCrypto { cipher: Some(cipher), enabled, required }
    }

    #[test]
    fn round_trip_and_format() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        let stored = dc.write_value(COL, "1234567").expect("暗号化できること");
        assert!(stored.starts_with("enc1:k1:"), "形式が enc1:<鍵ID>:…");
        assert!(!stored.contains("1234567"), "平文が残らない");
        assert_eq!(dc.read_value(COL, &stored).expect("復号できること"), "1234567");
        assert_eq!(DataCrypto::key_id_of(&stored), Some("k1"));
    }

    #[test]
    fn nonce_differs_every_time() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        let a = dc.encrypt(COL, "same").expect("暗号化");
        let b = dc.encrypt(COL, "same").expect("暗号化");
        assert_ne!(a, b, "同じ平文でも暗号文が毎回違う(nonce が毎回ランダム)");
    }

    #[test]
    fn aad_binds_the_column_name() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        let stored = dc.encrypt(COL, "1234567").expect("暗号化");
        // 別の列の値として入れ替えると、復号できない
        assert!(dc.read_value("m_employee_bank_account.account_number", &stored).is_err());
        assert!(dc.read_value("s_company_info.account_number", &stored).is_err());
    }

    #[test]
    fn tampering_is_detected() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        let stored = dc.encrypt(COL, "1234567").expect("暗号化");
        let (head, b64) = stored.rsplit_once(':').expect("形式");
        let mut blob = base64::engine::general_purpose::STANDARD.decode(b64).expect("base64");
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        let tampered = format!("{head}:{}", base64::engine::general_purpose::STANDARD.encode(blob));
        assert!(dc.read_value(COL, &tampered).is_err(), "改ざんは検知される");
    }

    #[test]
    fn plaintext_passes_unless_required() {
        let k = new_key_b64();
        let lenient = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        assert_eq!(lenient.read_value(COL, "1234567").expect("平文は通る(移行期間)"), "1234567");
        let strict = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, true);
        assert!(strict.read_value(COL, "1234567").is_err(), "REQUIRED では平文を拒否");
        assert!(strict.read_value(COL, "").is_err(), "空欄の平文も拒否(空でも暗号化して保存する)");
    }

    #[test]
    fn write_is_plaintext_while_disabled() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), false, false);
        assert_eq!(dc.write_value(COL, "1234567").expect("書ける"), "1234567");
    }

    #[test]
    fn rotation_reads_old_key_and_writes_new_key() {
        let old = new_key_b64();
        let new = new_key_b64();
        let before = dc_with(cipher_from(&keys_json("k1", &[("k1", &old)])), true, false);
        let stored = before.encrypt(COL, "1234567").expect("旧鍵で暗号化");

        let during = dc_with(cipher_from(&keys_json("k2", &[("k1", &old), ("k2", &new)])), true, false);
        assert_eq!(during.read_value(COL, &stored).expect("旧鍵の行も読める"), "1234567");
        let re = during.encrypt(COL, "1234567").expect("新鍵で暗号化");
        assert!(re.starts_with("enc1:k2:"));

        let after = dc_with(cipher_from(&keys_json("k2", &[("k2", &new)])), true, false);
        assert!(after.read_value(COL, &stored).is_err(), "古い鍵を消すと、旧鍵の行は読めない(分かるエラーになる)");
        assert_eq!(after.read_value(COL, &re).expect("新鍵の行は読める"), "1234567");
    }

    #[test]
    fn malformed_values_are_rejected() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        assert!(dc.decrypt(COL, "enc1:").is_err());
        assert!(dc.decrypt(COL, "enc1:k1").is_err());
        assert!(dc.decrypt(COL, "enc1::AAAA").is_err());
        assert!(dc.decrypt(COL, "enc1:k1:!!!").is_err());
        assert!(dc.decrypt(COL, "enc1:k1:AAAA").is_err(), "nonce より短い");
        assert!(dc.decrypt(COL, "plain").is_err());
    }

    #[test]
    fn fake_encrypted_looking_value_is_not_readable() {
        // 平文に見えるが enc1: で始まり、現行の鍵IDで始まる偽の値も、復号できない(変換の前に見つけられる)
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        assert!(DataCrypto::is_encrypted("enc1:k1:AAAAAAAAAAAAAAAAAAAAAAAAAAAA"));
        assert!(dc.decrypt(COL, "enc1:k1:AAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_err());
    }

    #[test]
    fn missing_key_fails_to_open_and_encrypt() {
        let dc = DataCrypto { cipher: None, enabled: false, required: false };
        assert!(dc.encrypt(COL, "x").is_err());
        assert!(dc.read_value(COL, "enc1:k1:AAAAAAAAAAAAAAAAAAAAAAAAAAAA").is_err());
        assert_eq!(dc.read_value(COL, "平文").expect("平文は通る"), "平文");
    }

    #[test]
    fn build_rules_by_environment() {
        let k = new_key_b64();
        let path = key_file("ok", &keys_json("k1", &[("k1", &k)]));

        // 本番: 鍵があれば有効
        let prod = DataCrypto::build("production", Some(path.as_str()), true, false).expect("本番で鍵が読めれば起動できる");
        assert!(prod.enabled() && prod.cipher().is_some());

        // 本番: 有効なのに鍵が無ければ起動を失敗させる
        assert!(DataCrypto::build("production", None, true, false).is_err());
        assert!(DataCrypto::build("staging", None, true, false).is_err());
        assert!(DataCrypto::build("dr", None, true, false).is_err());
        // 本番: 鍵ファイルを指定したのに読めなければ失敗
        assert!(DataCrypto::build("production", Some("/nonexistent/sophia/data-keys.json"), false, false).is_err());

        // 本番: 何も指定していなければ(移行前)起動できる。平文のまま
        let plain = DataCrypto::build("production", None, false, false).expect("移行前は起動できる");
        assert!(!plain.enabled() && plain.cipher().is_none());

        // 開発: 有効でも鍵が無ければ警告して無効
        let dev = DataCrypto::build("local", None, true, false).expect("開発は起動できる");
        assert!(!dev.enabled());
        // 開発: 鍵ファイルが読めなくても起動できる
        assert!(DataCrypto::build("local", Some("/nonexistent/x.json"), false, false).is_ok());

        // REQUIRED は ENABLED が無いと拒否、鍵が無くても拒否
        assert!(DataCrypto::build("production", Some(path.as_str()), false, true).is_err());
        assert!(DataCrypto::build("local", None, true, true).is_err());
        assert!(DataCrypto::build("production", Some(path.as_str()), true, true).is_ok());
    }

    #[test]
    fn unknown_environment_names_are_strict() {
        // 本番以外の名前(prod・空＝ENV_NAME 未設定)でも、有効化して鍵が無ければ起動を止める
        assert!(DataCrypto::build("prod", None, true, false).is_err());
        assert!(DataCrypto::build("", None, true, false).is_err());
        assert!(DataCrypto::build("", Some("/nonexistent/x.json"), false, false).is_err());
        // 開発の名前だけ、警告して無効にする
        assert!(DataCrypto::build("dev", None, true, false).is_ok());
        assert!(DataCrypto::build("test", None, true, false).is_ok());
    }

    #[test]
    fn flags_accept_only_known_values() {
        assert_eq!(parse_flag("X", None), Ok(false));
        assert_eq!(parse_flag("X", Some("")), Ok(false));
        assert_eq!(parse_flag("X", Some("false")), Ok(false));
        assert_eq!(parse_flag("X", Some("0")), Ok(false));
        assert_eq!(parse_flag("X", Some("no")), Ok(false));
        assert_eq!(parse_flag("X", Some("TRUE")), Ok(true));
        assert_eq!(parse_flag("X", Some(" 1 ")), Ok(true));
        assert_eq!(parse_flag("X", Some("yes")), Ok(true));
        assert!(parse_flag("X", Some("on")).is_err(), "不明な値は起動エラー");
        assert!(parse_flag("X", Some("ture")).is_err());
    }

    #[test]
    fn key_ids_are_validated() {
        let k = new_key_b64();
        let ok = key_file("id-ok", &keys_json("k-1_A", &[("k-1_A", &k)]));
        assert!(DataCrypto::build("production", Some(ok.as_str()), true, false).is_ok());

        let colon = key_file("id-colon", &keys_json("a:b", &[("a:b", &k)]));
        assert!(DataCrypto::build("production", Some(colon.as_str()), true, false).is_err(), ": を含む鍵IDは拒否");
        let long_id = "x".repeat(33);
        let long = key_file("id-long", &keys_json(&long_id, &[(long_id.as_str(), &k)]));
        assert!(DataCrypto::build("production", Some(long.as_str()), true, false).is_err(), "33 文字は拒否");
        let empty = key_file("id-empty", &keys_json("", &[("", &k)]));
        assert!(DataCrypto::build("production", Some(empty.as_str()), true, false).is_err(), "空は拒否");

        assert!(valid_key_id(&"x".repeat(32)));
        assert!(!valid_key_id(""));
        assert!(!valid_key_id("鍵"));
    }

    #[cfg(unix)]
    #[test]
    fn loose_key_file_permission_fails_in_production_only() {
        use std::os::unix::fs::PermissionsExt;
        let k = new_key_b64();
        let path = key_file("perm", &keys_json("k1", &[("k1", &k)]));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("権限を変える");
        assert!(DataCrypto::build("production", Some(path.as_str()), true, false).is_err(), "本番では権限が緩いと起動しない");
        assert!(DataCrypto::build("local", Some(path.as_str()), true, false).is_ok(), "開発では警告だけ");
    }

    #[test]
    fn payload_with_encrypted_value_is_refused() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        let stored = dc.encrypt(COL, "1234567").expect("暗号化");
        let json = format!("{{\"account_number\":\"{stored}\"}}");
        let err = ensure_no_encrypted_payload(&json).expect_err("暗号文が混ざれば止める");
        assert!(!err.contains(&stored) && !err.contains("1234567"), "エラーに値を出さない");
        // 空の平文の暗号文(nonce + タグだけ。最も短い形)も検知する
        let empty = dc.encrypt(COL, "").expect("暗号化");
        assert!(ensure_no_encrypted_payload(&format!("{{\"a\":\"{empty}\"}}")).is_err(), "空の暗号文も検知する");
        // 本文の中に埋もれた暗号文も検知する
        assert!(ensure_no_encrypted_payload(&format!("口座: {stored} です")).is_err());
        assert!(ensure_no_encrypted_payload("{\"account_number\":\"1234567\"}").is_ok(), "平文は通す");
        assert!(ensure_no_encrypted_payload("").is_ok());
    }

    #[test]
    fn payload_with_enc1_text_in_memos_is_allowed() {
        // 「enc1:」という字が文章に出てくるだけの JSON は通す(誤検知しない)
        assert!(ensure_no_encrypted_payload("{\"memo\":\"enc1: は暗号化の接頭辞です\"}").is_ok());
        assert!(ensure_no_encrypted_payload("{\"memo\":\"enc1:k1:短い\"}").is_ok(), "形に合わない短い値は通す");
        assert!(ensure_no_encrypted_payload("{\"memo\":\"ENC1:k1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}").is_ok(), "大文字は別の値");
        assert!(ensure_no_encrypted_payload("{\"memo\":\"enc1:bad key:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\"}").is_ok(), "鍵IDの形に合わない");
    }

    #[test]
    fn ciphertext_pattern_is_compiled() {
        std::sync::LazyLock::force(&CIPHERTEXT_RX);
    }

    #[test]
    fn debug_does_not_show_keys() {
        let k = new_key_b64();
        let dc = dc_with(cipher_from(&keys_json("k1", &[("k1", &k)])), true, false);
        let s = format!("{dc:?}");
        assert!(!s.contains(&k));
    }
}
