//! 「保存する」役割

use async_trait::async_trait;

use crate::check::Integration;

/// 保存したファイル
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    /// 保存先での ID
    pub id: String,
    /// 実際に付いたファイル名(`put_versioned` で `_v2` などが付くことがある)
    pub name: String,
    /// 人が開くための URL(無い保存先もある)
    pub url: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// 同じ名前のファイルがすでにある(`put_new`)
    #[error("同じ名前のファイルがすでにあります: {0}")]
    Conflict(String),
    /// 名前に使えない、版の上限を超えた など、渡された値の問題
    #[error("保存できない値です: {0}")]
    Invalid(String),
    /// 認証の失敗(鍵が読めない・トークンが取れない)
    #[error("認証に失敗しました: {0}")]
    Auth(String),
    /// 相手のサービスがエラーを返した
    #[error("保存先がエラーを返しました: {0}")]
    Remote(String),
    /// 通信の失敗
    #[error("保存先と通信できません: {0}")]
    Transport(String),
}

/// 保存する
///
/// `folder` は根からの道筋。無いフォルダは作る。
#[async_trait]
pub trait FileStore: Integration {
    /// 新しいファイルとして保存する。同じ名前があれば [`StoreError::Conflict`]。
    async fn put_new(&self, folder: &[&str], name: &str, bytes: &[u8], mime: &str)
        -> Result<StoredFile, StoreError>;

    /// 同じ名前があれば `<名前>_v2.<拡張子>`、`_v3` … と空いている名前を探して保存する。
    /// 前の版は残す(送り直し用)。
    async fn put_versioned(&self, folder: &[&str], name: &str, bytes: &[u8], mime: &str)
        -> Result<StoredFile, StoreError>;

    /// 同じ名前があれば中身を置き換える(受信した添付の保存など、上書きでよい用途だけに使う)。
    async fn put_overwrite(&self, folder: &[&str], name: &str, bytes: &[u8], mime: &str)
        -> Result<StoredFile, StoreError>;
}

/// `put_versioned` の n 版目の名前を作る。`n == 1` はそのまま。
/// 例: `PO-1_2026-10.pdf` → `PO-1_2026-10_v2.pdf`
pub fn versioned_name(name: &str, n: u32) -> String {
    if n <= 1 {
        return name.to_string();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{}_v{}{}", &name[..dot], n, &name[dot..]),
        _ => format!("{}_v{}", name, n),
    }
}

#[cfg(test)]
mod tests {
    use super::versioned_name;

    #[test]
    fn versioned_names() {
        assert_eq!(versioned_name("PO-1_2026-10.pdf", 1), "PO-1_2026-10.pdf");
        assert_eq!(versioned_name("PO-1_2026-10.pdf", 2), "PO-1_2026-10_v2.pdf");
        assert_eq!(versioned_name("a.b.pdf", 3), "a.b_v3.pdf");
        assert_eq!(versioned_name("noext", 2), "noext_v2");
        assert_eq!(versioned_name(".hidden", 2), ".hidden_v2");
    }
}
