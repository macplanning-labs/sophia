//! store-gdrive — 保存する部品: Google Drive
//!
//! - 共有ドライブに対応(`supportsAllDrives` / `includeItemsFromAllDrives`)。
//! - 必要な設定: `root_folder_id`(必須)、`service_account_json`(必須・秘密)、`impersonate`(任意)。
//! - 点検(書き込みなし): トークンが取れる / 根のフォルダが見える / 根に書き込める(`canAddChildren`)。
//! - 名前を Drive の検索の式に入れるときは `'` と `\` をエスケープする。
//! - HTTP のエラー(4xx/5xx)は必ずエラーとして返す(黙って成功扱いにしない)。

mod token;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use integration_core::{versioned_name, CheckReport, FileStore, Integration, SettingSpec, StoreError, StoredFile};
use serde::Deserialize;

pub use token::{ServiceAccountTokenSource, StaticToken, TokenSource};

pub const DRIVE_SCOPE: &str = "https://www.googleapis.com/auth/drive";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const DEFAULT_API_BASE: &str = "https://www.googleapis.com";
/// `put_versioned` で探す版の上限
const MAX_VERSIONS: u32 = 99;
const TIMEOUT: Duration = Duration::from_secs(60);

/// 設定
#[derive(Debug, Clone)]
pub struct GDriveConfig {
    /// 保存先の根のフォルダの ID
    pub root_folder_id: String,
    /// API の送り先(テストで偽のサーバへ向けるため。通常は既定値)
    pub api_base: String,
}

impl GDriveConfig {
    pub fn new(root_folder_id: impl Into<String>) -> Self {
        Self { root_folder_id: root_folder_id.into(), api_base: DEFAULT_API_BASE.to_string() }
    }
}

pub struct GDriveStore {
    cfg: GDriveConfig,
    tokens: Arc<dyn TokenSource>,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(rename = "webViewLink", default)]
    web_view_link: String,
}

#[derive(Debug, Deserialize)]
struct DriveFileList {
    #[serde(default)]
    files: Vec<DriveFile>,
}

#[derive(Debug, Deserialize)]
struct RootInfo {
    #[serde(rename = "mimeType", default)]
    mime_type: String,
    #[serde(default)]
    capabilities: Capabilities,
}

#[derive(Debug, Deserialize, Default)]
struct Capabilities {
    #[serde(rename = "canAddChildren", default)]
    can_add_children: bool,
}

/// Drive の検索の式に入れる文字列のエスケープ
pub fn escape_query(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn validate_name(kind: &str, name: &str) -> Result<(), StoreError> {
    if name.trim().is_empty() {
        return Err(StoreError::Invalid(format!("{kind}が空です")));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(StoreError::Invalid(format!("{kind}に制御文字が含まれています")));
    }
    Ok(())
}

/// 通信する前に、道筋とファイル名をまとめて確かめる(途中まで作ってから失敗しない)
fn validate_target(folder: &[&str], name: &str) -> Result<(), StoreError> {
    for f in folder {
        validate_name("フォルダ名", f)?;
    }
    validate_name("ファイル名", name)
}

fn file_url(f: &DriveFile) -> String {
    if f.web_view_link.is_empty() {
        format!("https://drive.google.com/file/d/{}/view", f.id)
    } else {
        f.web_view_link.clone()
    }
}

fn transport(e: reqwest::Error) -> StoreError {
    StoreError::Transport(e.without_url().to_string())
}

/// 2xx 以外をエラーにする。本文は長すぎないように切る(トークンは本文に含まれない)
async fn ensure_success(resp: reqwest::Response, what: &str) -> Result<reqwest::Response, StoreError> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    let body: String = body.chars().take(300).collect();
    let msg = format!("{what}: HTTP {status} {body}");
    if status == reqwest::StatusCode::UNAUTHORIZED {
        Err(StoreError::Auth(msg))
    } else {
        Err(StoreError::Remote(msg))
    }
}

impl GDriveStore {
    pub const SETTINGS: &'static [SettingSpec] = &[
        SettingSpec { key: "root_folder_id", label_ja: "保存先の根のフォルダの ID", secret: false, required: true },
        SettingSpec { key: "service_account_json", label_ja: "サービスアカウントの鍵(JSON)", secret: true, required: true },
        SettingSpec { key: "impersonate", label_ja: "なりすまし先のメールアドレス(ドメイン全体の委任)", secret: false, required: false },
    ];

    pub fn new(cfg: GDriveConfig, tokens: Arc<dyn TokenSource>) -> Result<Self, StoreError> {
        validate_name("根のフォルダの ID", &cfg.root_folder_id)?;
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .build()
            .map_err(|e| StoreError::Invalid(format!("HTTP クライアントを作れません: {e}")))?;
        Ok(Self { cfg, tokens, client })
    }

    fn files_url(&self) -> String {
        format!("{}/drive/v3/files", self.cfg.api_base)
    }

    async fn token(&self) -> Result<String, StoreError> {
        self.tokens.token(DRIVE_SCOPE).await
    }

    /// 親フォルダの直下から、名前が一致するもの(フォルダかファイル)を1つ探す
    async fn find_child(&self, token: &str, parent: &str, name: &str, folder: bool) -> Result<Option<DriveFile>, StoreError> {
        let kind = if folder {
            format!(" and mimeType='{FOLDER_MIME}'")
        } else {
            format!(" and mimeType!='{FOLDER_MIME}'")
        };
        let q = format!(
            "name='{}' and '{}' in parents and trashed=false{kind}",
            escape_query(name),
            escape_query(parent)
        );
        let resp = self
            .client
            .get(self.files_url())
            .bearer_auth(token)
            .query(&[
                ("q", q.as_str()),
                ("fields", "files(id,name,webViewLink)"),
                ("pageSize", "1"),
                ("supportsAllDrives", "true"),
                ("includeItemsFromAllDrives", "true"),
            ])
            .send()
            .await
            .map_err(transport)?;
        let list: DriveFileList = ensure_success(resp, "検索").await?.json().await.map_err(transport)?;
        Ok(list.files.into_iter().next())
    }

    async fn create_folder(&self, token: &str, parent: &str, name: &str) -> Result<String, StoreError> {
        let meta = serde_json::json!({ "name": name, "mimeType": FOLDER_MIME, "parents": [parent] });
        let resp = self
            .client
            .post(self.files_url())
            .bearer_auth(token)
            .query(&[("supportsAllDrives", "true"), ("fields", "id")])
            .json(&meta)
            .send()
            .await
            .map_err(transport)?;
        let f: DriveFile = ensure_success(resp, "フォルダの作成").await?.json().await.map_err(transport)?;
        Ok(f.id)
    }

    /// 根からの道筋をたどり、無いフォルダは作って、最後のフォルダの ID を返す
    async fn ensure_folder_path(&self, token: &str, folder: &[&str]) -> Result<String, StoreError> {
        let mut parent = self.cfg.root_folder_id.clone();
        for name in folder {
            parent = match self.find_child(token, &parent, name, true).await? {
                Some(f) => f.id,
                None => self.create_folder(token, &parent, name).await?,
            };
        }
        Ok(parent)
    }

    async fn create_file(&self, token: &str, parent: &str, name: &str, bytes: &[u8], mime: &str) -> Result<StoredFile, StoreError> {
        let meta = serde_json::json!({ "name": name, "parents": [parent] });
        let form = reqwest::multipart::Form::new()
            .part(
                "metadata",
                reqwest::multipart::Part::text(meta.to_string())
                    .mime_str("application/json")
                    .map_err(|e| StoreError::Invalid(e.to_string()))?,
            )
            .part(
                "file",
                reqwest::multipart::Part::bytes(bytes.to_vec())
                    .file_name(name.to_string())
                    .mime_str(mime)
                    .map_err(|e| StoreError::Invalid(format!("MIME の形が正しくありません: {e}")))?,
            );
        let resp = self
            .client
            .post(format!("{}/upload/drive/v3/files", self.cfg.api_base))
            .bearer_auth(token)
            .query(&[("uploadType", "multipart"), ("supportsAllDrives", "true"), ("fields", "id,name,webViewLink")])
            .multipart(form)
            .send()
            .await
            .map_err(transport)?;
        let f: DriveFile = ensure_success(resp, "アップロード").await?.json().await.map_err(transport)?;
        Ok(StoredFile { url: Some(file_url(&f)), name: if f.name.is_empty() { name.to_string() } else { f.name.clone() }, id: f.id })
    }

    async fn update_content(&self, token: &str, existing: &DriveFile, bytes: &[u8], mime: &str) -> Result<StoredFile, StoreError> {
        let resp = self
            .client
            .patch(format!("{}/upload/drive/v3/files/{}", self.cfg.api_base, existing.id))
            .bearer_auth(token)
            .query(&[("uploadType", "media"), ("supportsAllDrives", "true"), ("fields", "id,name,webViewLink")])
            .header(reqwest::header::CONTENT_TYPE, mime)
            .body(bytes.to_vec())
            .send()
            .await
            .map_err(transport)?;
        let f: DriveFile = ensure_success(resp, "上書き").await?.json().await.map_err(transport)?;
        Ok(StoredFile { url: Some(file_url(&f)), name: existing.name.clone(), id: f.id })
    }
}

#[derive(Debug, Deserialize)]
struct NameOnly {
    #[serde(default)]
    name: String,
}

impl GDriveStore {
    /// 保存先の根のフォルダの名前(画面に「どのフォルダか」を出すため。書き込みなし)
    pub async fn folder_name(&self) -> Result<String, StoreError> {
        let token = self.token().await?;
        let resp = self
            .client
            .get(format!("{}/{}", self.files_url(), self.cfg.root_folder_id))
            .bearer_auth(&token)
            .query(&[("supportsAllDrives", "true"), ("fields", "name")])
            .send()
            .await
            .map_err(transport)?;
        let n: NameOnly = ensure_success(resp, "フォルダ名の取得").await?.json().await.map_err(transport)?;
        Ok(n.name)
    }
}

#[async_trait]
impl Integration for GDriveStore {
    fn provider(&self) -> &'static str {
        "gdrive"
    }

    async fn check(&self) -> CheckReport {
        let r = CheckReport::new();
        let token = match self.token().await {
            Ok(t) => t,
            Err(e) => return r.fail("サービスアカウントでトークンが取れる", e.to_string()),
        };
        let r = r.pass("サービスアカウントでトークンが取れる");
        let resp = self
            .client
            .get(format!("{}/{}", self.files_url(), self.cfg.root_folder_id))
            .bearer_auth(&token)
            .query(&[("supportsAllDrives", "true"), ("fields", "id,mimeType,capabilities(canAddChildren)")])
            .send()
            .await;
        let info: RootInfo = match resp {
            Err(e) => return r.fail("保存先の根のフォルダが見える", transport(e).to_string()),
            Ok(resp) => match ensure_success(resp, "根のフォルダの取得").await {
                Err(e) => {
                    return r.fail(
                        "保存先の根のフォルダが見える",
                        format!("{e}(フォルダ ID の誤りか、サービスアカウントが共有ドライブのメンバーでない可能性)"),
                    )
                }
                Ok(resp) => match resp.json().await {
                    Ok(i) => i,
                    Err(e) => return r.fail("保存先の根のフォルダが見える", transport(e).to_string()),
                },
            },
        };
        let r = if info.mime_type == FOLDER_MIME {
            r.pass("保存先の根のフォルダが見える")
        } else {
            return r.fail("保存先の根のフォルダが見える", format!("フォルダではありません({})", info.mime_type));
        };
        if info.capabilities.can_add_children {
            r.pass("保存先の根に書き込める")
        } else {
            r.fail("保存先の根に書き込める", "書き込む権限がありません(共有ドライブで「コンテンツ管理者」以上が必要)")
        }
    }
}

#[async_trait]
impl FileStore for GDriveStore {
    async fn put_new(&self, folder: &[&str], name: &str, bytes: &[u8], mime: &str) -> Result<StoredFile, StoreError> {
        validate_target(folder, name)?;
        let token = self.token().await?;
        let parent = self.ensure_folder_path(&token, folder).await?;
        if self.find_child(&token, &parent, name, false).await?.is_some() {
            return Err(StoreError::Conflict(name.to_string()));
        }
        self.create_file(&token, &parent, name, bytes, mime).await
    }

    async fn put_versioned(&self, folder: &[&str], name: &str, bytes: &[u8], mime: &str) -> Result<StoredFile, StoreError> {
        validate_target(folder, name)?;
        let token = self.token().await?;
        let parent = self.ensure_folder_path(&token, folder).await?;
        for n in 1..=MAX_VERSIONS {
            let candidate = versioned_name(name, n);
            if self.find_child(&token, &parent, &candidate, false).await?.is_none() {
                return self.create_file(&token, &parent, &candidate, bytes, mime).await;
            }
        }
        Err(StoreError::Invalid(format!("{name} の版が上限({MAX_VERSIONS})を超えました")))
    }

    async fn put_overwrite(&self, folder: &[&str], name: &str, bytes: &[u8], mime: &str) -> Result<StoredFile, StoreError> {
        validate_target(folder, name)?;
        let token = self.token().await?;
        let parent = self.ensure_folder_path(&token, folder).await?;
        match self.find_child(&token, &parent, name, false).await? {
            Some(existing) => self.update_content(&token, &existing, bytes, mime).await,
            None => self.create_file(&token, &parent, name, bytes, mime).await,
        }
    }
}

#[cfg(test)]
mod tests;
