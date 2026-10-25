//! 部品が持つ「必要な設定」と「点検」

use async_trait::async_trait;

/// 部品が動くのに必要な設定項目の定義(値は持たない)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingSpec {
    /// 設定の名前(例: `root_folder_id`)
    pub key: &'static str,
    /// 画面やログに出す日本語の名前
    pub label_ja: &'static str,
    /// 秘密の値か(秘密の値は画面・ログ・点検結果に出さない)
    pub secret: bool,
    /// 必須か
    pub required: bool,
}

/// 点検の1項目の結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckItem {
    /// 何を確かめたか(例: 「保存先のフォルダが見える」)
    pub name: String,
    pub ok: bool,
    /// NG の理由など。秘密の値は含めない
    pub detail: Option<String>,
}

/// 点検の結果
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CheckReport {
    pub items: Vec<CheckItem>,
}

impl CheckReport {
    pub fn new() -> Self {
        Self::default()
    }

    /// 全項目が OK か(項目が無いときは false。何も確かめていないものを正常と言わない)
    pub fn ok(&self) -> bool {
        !self.items.is_empty() && self.items.iter().all(|i| i.ok)
    }

    pub fn pass(mut self, name: impl Into<String>) -> Self {
        self.items.push(CheckItem { name: name.into(), ok: true, detail: None });
        self
    }

    pub fn fail(mut self, name: impl Into<String>, detail: impl Into<String>) -> Self {
        self.items.push(CheckItem { name: name.into(), ok: false, detail: Some(detail.into()) });
        self
    }

    /// NG の項目を「名前: 理由」の形で並べる(ログ・通知用)
    pub fn failures(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|i| !i.ok)
            .map(|i| match &i.detail {
                Some(d) => format!("{}: {}", i.name, d),
                None => i.name.clone(),
            })
            .collect()
    }
}

/// どの部品も持つ: つなぎ先の名前と点検
#[async_trait]
pub trait Integration: Send + Sync {
    /// つなぎ先の名前(例: `gdrive`、`gchat`)
    fn provider(&self) -> &'static str;

    /// 相手に何も書き込まない点検(状態を変えない)
    async fn check(&self) -> CheckReport;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_is_not_ok() {
        assert!(!CheckReport::new().ok());
    }

    #[test]
    fn report_ok_only_when_all_pass() {
        let r = CheckReport::new().pass("a").pass("b");
        assert!(r.ok());
        let r = r.fail("c", "理由");
        assert!(!r.ok());
        assert_eq!(r.failures(), vec!["c: 理由".to_string()]);
    }
}
