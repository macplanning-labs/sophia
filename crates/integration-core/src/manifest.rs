//! 使う部品の一覧(Manifest)と、実際の状態との照合
//!
//! アプリは `integrations.toml` に「使う用途」を宣言する。
//!
//! ```toml
//! [[use]]
//! id = "ops_alert"
//! capability = "notify"
//! required = true
//! fallback = "mail_admin"
//! description = "異常の通知"
//! ```

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

/// 役割の名前として受け付けるもの
const KNOWN_CAPABILITIES: &[&str] = &["notify", "file_store", "mail_send", "mail_receive", "server_job"];

/// 用途1つの宣言
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UseDecl {
    /// 用途の ID(例: `ops_alert`)
    pub id: String,
    /// 役割(`notify` / `file_store` / `mail_send` / `mail_receive` / `server_job`)
    pub capability: String,
    /// 必須か(省略時は必須)
    #[serde(default = "default_true")]
    pub required: bool,
    /// この用途が使えないときの代わりの手段(例: `mail_admin`)
    #[serde(default)]
    pub fallback: Option<String>,
    /// サーバの仕事(`server_job`)を確かめるスクリプトなど
    #[serde(default)]
    pub check: Option<String>,
    /// 人が読む説明
    #[serde(default)]
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

/// 使う部品の一覧
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(rename = "use", default)]
    pub uses: Vec<UseDecl>,
}

#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("使う部品の一覧を読めません: {0}")]
    Parse(String),
    #[error("用途の ID が重複しています: {0}")]
    DuplicateId(String),
    #[error("知らない役割です: {id} の capability = {capability}")]
    UnknownCapability { id: String, capability: String },
    #[error("用途の ID が空です")]
    EmptyId,
}

impl Manifest {
    /// TOML を読み、重複・知らない役割が無いことを確かめる
    pub fn from_toml(s: &str) -> Result<Self, ManifestError> {
        let m: Manifest = toml::from_str(s).map_err(|e| ManifestError::Parse(e.to_string()))?;
        let mut seen = HashSet::new();
        for u in &m.uses {
            if u.id.trim().is_empty() {
                return Err(ManifestError::EmptyId);
            }
            if !seen.insert(u.id.as_str()) {
                return Err(ManifestError::DuplicateId(u.id.clone()));
            }
            if !KNOWN_CAPABILITIES.contains(&u.capability.as_str()) {
                return Err(ManifestError::UnknownCapability { id: u.id.clone(), capability: u.capability.clone() });
            }
        }
        Ok(m)
    }

    pub fn get(&self, id: &str) -> Option<&UseDecl> {
        self.uses.iter().find(|u| u.id == id)
    }
}

/// 用途1つの状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseStatus {
    /// 設定があり、点検も OK
    Ok,
    /// 設定が無い
    Missing(String),
    /// 設定はあるが、点検が NG(理由の一覧)
    Failed(Vec<String>),
    /// ここでは確かめない(例: サーバの仕事はデプロイ前の点検で確かめる)
    Deferred(String),
}

/// 照合の結果(用途1つ分)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseReport {
    pub decl: UseDecl,
    pub status: UseStatus,
}

impl UseReport {
    /// 必須なのに使えない状態か(起動時・毎朝は知らせる、デプロイ前は止める対象)
    pub fn is_problem(&self) -> bool {
        self.decl.required && matches!(self.status, UseStatus::Missing(_) | UseStatus::Failed(_))
    }

    /// ログ・通知用の1行(秘密の値は含まない前提の文だけを使う)
    pub fn summary_ja(&self) -> String {
        let head = match &self.decl.description {
            Some(d) => format!("{}［{}］{}", self.decl.id, if self.decl.required { "必須" } else { "任意" }, d),
            None => format!("{}［{}］", self.decl.id, if self.decl.required { "必須" } else { "任意" }),
        };
        match &self.status {
            UseStatus::Ok => format!("正常   {head}"),
            UseStatus::Missing(why) => format!("未設定 {head} → {why}"),
            UseStatus::Failed(items) => format!("異常   {head} → {}", items.join(" / ")),
            UseStatus::Deferred(why) => format!("対象外 {head} → {why}"),
        }
    }
}

impl UseReport {
    /// 画面の帯・通知の本文に出す、人が読む1行(用途の ID や「必須」の記号を出さない)。
    /// 問題でない状態(正常・対象外)は None。
    pub fn plain_ja(&self) -> Option<String> {
        let label = self.decl.description.clone().unwrap_or_else(|| self.decl.id.clone());
        match &self.status {
            UseStatus::Missing(why) => Some(format!("{label} が設定されていません（{why}）")),
            UseStatus::Failed(items) => Some(format!("{label} が使えません（{}）", items.join(" / "))),
            UseStatus::Ok | UseStatus::Deferred(_) => None,
        }
    }
}

/// 一覧と、アプリが調べた用途ごとの状態を照らし合わせる。
/// `statuses` に無い用途は「未設定」とみなす(調べ忘れを正常扱いしない)。
pub fn evaluate(manifest: &Manifest, statuses: &HashMap<String, UseStatus>) -> Vec<UseReport> {
    manifest
        .uses
        .iter()
        .map(|u| UseReport {
            decl: u.clone(),
            status: statuses
                .get(&u.id)
                .cloned()
                .unwrap_or_else(|| UseStatus::Missing("状態を調べていません".to_string())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_ja_hides_ids_and_marks() {
        let decl = UseDecl {
            id: "mail_send".into(),
            capability: "mail_send".into(),
            required: true,
            fallback: None,
            check: None,
            description: Some("メール送信".into()),
        };
        let missing = UseReport { decl: decl.clone(), status: UseStatus::Missing("鍵のファイルがありません".into()) };
        assert_eq!(missing.plain_ja().unwrap(), "メール送信 が設定されていません（鍵のファイルがありません）");
        let failed = UseReport { decl: decl.clone(), status: UseStatus::Failed(vec!["a".into(), "b".into()]) };
        assert_eq!(failed.plain_ja().unwrap(), "メール送信 が使えません（a / b）");
        assert!(UseReport { decl: decl.clone(), status: UseStatus::Ok }.plain_ja().is_none());
        assert!(UseReport { decl, status: UseStatus::Deferred("x".into()) }.plain_ja().is_none());
    }

    const SAMPLE: &str = r#"
[[use]]
id = "ops_alert"
capability = "notify"
fallback = "mail_admin"
description = "異常の通知"

[[use]]
id = "db_backup"
capability = "server_job"
check = "scripts/check_backup_age.sh"

[[use]]
id = "sheets"
capability = "file_store"
required = false
"#;

    #[test]
    fn parses_with_defaults() {
        let m = Manifest::from_toml(SAMPLE).unwrap();
        assert_eq!(m.uses.len(), 3);
        assert!(m.get("ops_alert").unwrap().required, "省略時は必須");
        assert_eq!(m.get("ops_alert").unwrap().fallback.as_deref(), Some("mail_admin"));
        assert!(!m.get("sheets").unwrap().required);
    }

    #[test]
    fn rejects_duplicate_unknown_and_typo() {
        let dup = "[[use]]\nid='a'\ncapability='notify'\n[[use]]\nid='a'\ncapability='notify'\n";
        assert!(matches!(Manifest::from_toml(dup), Err(ManifestError::DuplicateId(_))));
        let unk = "[[use]]\nid='a'\ncapability='fax'\n";
        assert!(matches!(Manifest::from_toml(unk), Err(ManifestError::UnknownCapability { .. })));
        let typo = "[[use]]\nid='a'\ncapability='notify'\nrequird=false\n";
        assert!(matches!(Manifest::from_toml(typo), Err(ManifestError::Parse(_))), "書き間違いを黙って無視しない");
        let empty = "[[use]]\nid=' '\ncapability='notify'\n";
        assert!(matches!(Manifest::from_toml(empty), Err(ManifestError::EmptyId)));
    }

    #[test]
    fn evaluate_marks_unreported_as_missing() {
        let m = Manifest::from_toml(SAMPLE).unwrap();
        let mut st = HashMap::new();
        st.insert("db_backup".to_string(), UseStatus::Deferred("デプロイ前に確かめる".into()));
        st.insert("sheets".to_string(), UseStatus::Missing("未設定".into()));
        let r = evaluate(&m, &st);
        let ops = r.iter().find(|u| u.decl.id == "ops_alert").unwrap();
        assert!(matches!(ops.status, UseStatus::Missing(_)));
        assert!(ops.is_problem());
        assert!(!r.iter().find(|u| u.decl.id == "db_backup").unwrap().is_problem(), "対象外は問題にしない");
        assert!(!r.iter().find(|u| u.decl.id == "sheets").unwrap().is_problem(), "任意の未設定は問題にしない");
    }

    #[test]
    fn failed_required_is_problem() {
        let m = Manifest::from_toml(SAMPLE).unwrap();
        let mut st = HashMap::new();
        st.insert("ops_alert".to_string(), UseStatus::Failed(vec!["URL の形: NG".into()]));
        let r = evaluate(&m, &st);
        assert!(r[0].is_problem());
        assert!(r[0].summary_ja().contains("異常"));
    }
}
