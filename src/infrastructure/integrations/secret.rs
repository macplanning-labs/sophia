//! 外部連携の秘密の設定(Webhook の URL・サービスアカウントの鍵)の暗号化(DEMO-000215。設計書 3.1 / D8)
//!
//! - ログイン用の `SECRET_KEY` とは別の、専用の鍵を使う(ログイン用の鍵を入れ替えても、外部連携の設定が読めなくならないように)。
//! - 鍵のファイル: 環境変数 `INTEGRATION_KEYS_FILE` の場所(サーバでは `~/sophia-secrets/<環境>/integration-keys.json` を
//!   コンテナに読み取り専用で渡す)。中身: `{"current": "<鍵ID>", "keys": {"<鍵ID>": "<32バイトの base64>"}}`
//! - 暗号: AES-256-GCM。行ごとに乱数の nonce。どの鍵で暗号化したかを鍵ID で残す。
//! - 鍵の入れ替え: 新しい鍵を keys に足して current を変え、`sophia rekey-integrations` で全行を暗号化し直してから、古い鍵を消す。

use std::collections::HashMap;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use rand::RngCore;
use serde::Deserialize;

#[derive(Deserialize)]
struct KeysFile {
    current: String,
    keys: HashMap<String, String>,
}

/// 暗号化した秘密
pub struct Sealed {
    pub key_id: String,
    pub nonce: Vec<u8>,
    pub ciphertext: Vec<u8>,
}

/// 鍵の束(Debug で鍵を出さない)
pub struct SecretCipher {
    current: String,
    keys: HashMap<String, [u8; 32]>,
}

impl std::fmt::Debug for SecretCipher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretCipher").field("current", &self.current).field("keys", &self.keys.len()).finish()
    }
}

impl SecretCipher {
    /// 鍵ファイルの JSON から作る
    pub fn from_json(s: &str) -> Result<Self, String> {
        let f: KeysFile = serde_json::from_str(s).map_err(|e| format!("暗号化の鍵のファイルを読めません: {e}"))?;
        let mut keys = HashMap::new();
        for (id, b64) in f.keys {
            let raw = base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|_| format!("暗号化の鍵 {id} が base64 ではありません"))?;
            let arr: [u8; 32] = raw.try_into().map_err(|_| format!("暗号化の鍵 {id} が 32 バイトではありません"))?;
            keys.insert(id, arr);
        }
        if !keys.contains_key(&f.current) {
            return Err(format!("今の鍵 {} が keys にありません", f.current));
        }
        Ok(Self { current: f.current, keys })
    }

    /// 環境変数 `INTEGRATION_KEYS_FILE` の場所から読む。未設定なら Err(理由)
    pub fn from_env() -> Result<Self, String> {
        let path = std::env::var("INTEGRATION_KEYS_FILE")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or_else(|| "暗号化の鍵(INTEGRATION_KEYS_FILE)が設定されていません".to_string())?;
        let s = std::fs::read_to_string(&path).map_err(|_| "暗号化の鍵のファイルを読めません(INTEGRATION_KEYS_FILE)".to_string())?;
        Self::from_json(&s)
    }

    /// 指定したファイル(鍵の JSON)から作る。データ暗号化(`data_crypto`)も使う
    pub fn from_file(path: &str) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|_| "暗号化の鍵のファイルを読めません".to_string())?;
        Self::from_json(&s)
    }

    pub fn current_key_id(&self) -> &str {
        &self.current
    }

    /// 鍵ファイルにある全ての鍵ID(形式の検証に使う。鍵の中身は返さない)
    pub fn key_ids(&self) -> Vec<&str> {
        self.keys.keys().map(|k| k.as_str()).collect()
    }

    pub fn seal(&self, plain: &[u8]) -> Result<Sealed, String> {
        self.seal_with_aad(plain, &[])
    }

    pub fn open(&self, key_id: &str, nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
        self.open_with_aad(key_id, nonce, ciphertext, &[])
    }

    /// 追加認証データ(AAD)つきで暗号化する。AAD が違うと復号できない(列の入れ替えを検知するため、列名を入れる)
    pub fn seal_with_aad(&self, plain: &[u8], aad: &[u8]) -> Result<Sealed, String> {
        let key = self.keys.get(&self.current).ok_or("今の鍵がありません")?;
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "鍵の長さが正しくありません".to_string())?;
        let mut nonce = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), aes_gcm::aead::Payload { msg: plain, aad })
            .map_err(|_| "暗号化に失敗しました".to_string())?;
        Ok(Sealed { key_id: self.current.clone(), nonce: nonce.to_vec(), ciphertext })
    }

    /// AAD つきで復号する。鍵が無い・AAD が違う・改ざんされている場合は Err
    pub fn open_with_aad(&self, key_id: &str, nonce: &[u8], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
        let key = self
            .keys
            .get(key_id)
            .ok_or_else(|| format!("暗号化に使った鍵 {key_id} がありません(鍵を入れ替えた後に古い鍵を消した可能性)"))?;
        if nonce.len() != 12 {
            return Err("暗号の nonce の長さが正しくありません".into());
        }
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| "鍵の長さが正しくありません".to_string())?;
        cipher
            .decrypt(Nonce::from_slice(nonce), aes_gcm::aead::Payload { msg: ciphertext, aad })
            .map_err(|_| "復号できません(鍵が違うか、データが壊れています)".to_string())
    }
}

/// 新しい鍵(32 バイト)を base64 で作る(鍵ファイルを作るときの手順書用)
pub fn new_key_b64() -> String {
    let mut k = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut k);
    base64::engine::general_purpose::STANDARD.encode(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys_json(current: &str, ids: &[&str]) -> String {
        let keys: Vec<String> = ids
            .iter()
            .map(|id| format!("\"{id}\": \"{}\"", base64::engine::general_purpose::STANDARD.encode([id.len() as u8; 32])))
            .collect();
        format!("{{\"current\": \"{current}\", \"keys\": {{{}}}}}", keys.join(","))
    }

    #[test]
    fn seal_and_open_round_trip() {
        let c = SecretCipher::from_json(&keys_json("k1", &["k1"])).expect("鍵を読めること");
        let s = c.seal(b"https://chat.googleapis.com/x?key=SECRET").expect("暗号化できること");
        assert_eq!(s.key_id, "k1");
        assert!(!s.ciphertext.windows(6).any(|w| w == b"SECRET"), "平文が残らない");
        assert_eq!(c.open(&s.key_id, &s.nonce, &s.ciphertext).expect("復号できること"), b"https://chat.googleapis.com/x?key=SECRET");
    }

    #[test]
    fn old_key_still_opens_after_rotation_and_missing_key_fails() {
        let old = SecretCipher::from_json(&keys_json("k1", &["k1"])).expect("鍵を読めること");
        let s = old.seal(b"x").expect("暗号化できること");
        let rotated = SecretCipher::from_json(&keys_json("k22", &["k1", "k22"])).expect("鍵を読めること");
        assert_eq!(rotated.open("k1", &s.nonce, &s.ciphertext).expect("古い鍵の行も読める"), b"x");
        assert_eq!(rotated.seal(b"y").expect("暗号化できること").key_id, "k22");
        let only_new = SecretCipher::from_json(&keys_json("k22", &["k22"])).expect("鍵を読めること");
        assert!(only_new.open("k1", &s.nonce, &s.ciphertext).is_err());
    }

    #[test]
    fn rejects_bad_files() {
        assert!(SecretCipher::from_json("{}").is_err());
        assert!(SecretCipher::from_json(&keys_json("k9", &["k1"])).is_err(), "今の鍵が無い");
        assert!(SecretCipher::from_json(r#"{"current":"k1","keys":{"k1":"c2hvcnQ="}}"#).is_err(), "32バイトでない");
        assert!(!format!("{:?}", SecretCipher::from_json(&keys_json("k1", &["k1"])).expect("鍵を読めること")).contains("AAAA"));
    }

    #[test]
    fn new_key_is_32_bytes() {
        let k = base64::engine::general_purpose::STANDARD.decode(new_key_b64()).expect("base64");
        assert_eq!(k.len(), 32);
    }
}
