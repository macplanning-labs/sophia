//! infrastructure/integrations — 外部連携の部品の組み立て（DEMO-000210 / 000213 / 000215）
//!
//! 設定を読み、社内共通リポジトリの部品（store-gdrive・notify-gchat）を組み立てて持つ。
//! 業務のコードは「知らせる」（[`notify_ops`]）「保存する」（[`file_store`]）だけを使い、
//! どこにつながっているかは知らない。
//!
//! - 設定は、画面で保存したもの（DB の s_integration_target）を優先し、用途に行が無ければ、サーバの設定ファイル
//!   （環境変数）を読む（移行の間の予備。設計書 3.1）。秘密の設定は専用の鍵で暗号化して DB に置く（[`secret`]）。
//! - 使う用途の一覧は、リポジトリ直下の `integrations.toml`（バイナリに埋め込む）。
//! - 起動時と、画面で設定を変えたときに [`reload`] で組み立て直す。

pub mod check;
pub mod secret;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use chrono::{DateTime, Utc};
use integration_core::{CompositeNotifier, FileStore, Manifest, Notice, Notifier};
use notify_gchat::GChatNotifier;
use sqlx::PgPool;
use store_gdrive::{GDriveConfig, GDriveStore, ServiceAccountTokenSource};

use crate::infrastructure::mail_pipeline::ops_message;
use crate::infrastructure::repositories::integration_repo::{self, TargetRow};

/// 使う用途の一覧（リポジトリ直下の integrations.toml）
const MANIFEST_TOML: &str = include_str!("../../../integrations.toml");

/// 画面で設定できる用途: 通知（送り先を複数にできる）と、保存先（1つ）
pub const NOTIFY_USE: &str = "ops_alert";
pub const STORE_USE: &str = "issued_pdf_archive";

/// 設定をどこから読んだか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// 画面で保存したもの（DB）
    Db,
    /// サーバの設定ファイル（環境変数。移行の間の予備）
    Env,
}

/// 組み立てた送り先（通知）
pub struct NotifyTarget {
    pub id: Option<i64>,
    pub label: String,
    pub provider: &'static str,
    pub source: Source,
    /// 宛先の ID(スペース ID。鍵の部分は含まない)
    pub destination: String,
    /// 宛先の名前(画面で登録したスペース名)。無ければ None
    pub destination_name: Option<String>,
    pub notifier: Arc<GChatNotifier>,
}

/// 組み立てた保存先
pub struct StoreTarget {
    pub id: Option<i64>,
    pub label: String,
    pub provider: &'static str,
    pub source: Source,
    /// 宛先の ID(フォルダ ID)
    pub destination: String,
    /// 宛先の名前(画面の設定に入っているフォルダ名)。無ければ点検のときに Drive から取る
    pub destination_name: Option<String>,
    pub store: Arc<GDriveStore>,
}

/// 設定はあるが、組み立てられなかった送り先（鍵が無い・復号できない・形が違う など）
#[derive(Debug, Clone)]
pub struct Broken {
    pub id: Option<i64>,
    pub label: String,
    pub reason: String,
}

/// 組み立て済みの部品
pub struct Registry {
    pub manifest: Manifest,
    pub notify: Vec<NotifyTarget>,
    pub notify_broken: Vec<Broken>,
    notify_missing: String,
    composite: Option<CompositeNotifier>,
    pub store: Option<StoreTarget>,
    pub store_broken: Option<Broken>,
    store_missing: String,
    pool: Option<PgPool>,
}

static REGISTRY: RwLock<Option<Arc<Registry>>> = RwLock::new(None);

/// 組み立て済みの部品。まだ組み立てていなければ、環境変数だけで組み立てる（起動直後・コマンド用）
pub fn registry() -> Arc<Registry> {
    if let Ok(g) = REGISTRY.read() {
        if let Some(r) = g.as_ref() {
            return r.clone();
        }
    }
    let r = Arc::new(Registry::build(&[], None, None));
    if let Ok(mut g) = REGISTRY.write() {
        if g.is_none() {
            *g = Some(r.clone());
        }
    }
    r
}

/// DB の設定（と環境変数）から組み立て直す。起動時と、画面で設定を変えたときに呼ぶ
pub async fn reload(pool: &PgPool) {
    let rows = match integration_repo::list_enabled_rows(pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[外部連携] 画面の設定を読めません。サーバの設定ファイルだけで組み立てます: {e}");
            Vec::new()
        }
    };
    let cipher = secret::SecretCipher::from_env();
    let r = Arc::new(Registry::build(&rows, cipher.as_ref().ok(), Some(pool.clone())));
    if let Ok(mut g) = REGISTRY.write() {
        *g = Some(r);
    }
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

fn load_manifest() -> Manifest {
    match Manifest::from_toml(MANIFEST_TOML) {
        Ok(m) => m,
        Err(e) => {
            // 埋め込みの一覧はテストで検査している。ここに来るのは想定外なので、止めずに大きく記録する
            tracing::error!("[外部連携] 使う部品の一覧（integrations.toml）を読めません: {e}");
            Manifest { uses: Vec::new() }
        }
    }
}

/// Webhook の URL から、宛先の表示（スペース ID。鍵の部分は出さない）
pub fn chat_destination(url: &str) -> String {
    url.split("/spaces/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .map(str::to_string)
        .unwrap_or_else(|| "（宛先を読み取れません）".to_string())
}

/// 秘密の設定(JSON)を復号して読む
fn open_secret(row: &TargetRow, cipher: Option<&secret::SecretCipher>) -> Result<serde_json::Value, String> {
    let (Some(enc), Some(nonce), Some(kid)) = (&row.secret_enc, &row.secret_nonce, &row.secret_key_id) else {
        return Err("秘密の設定がありません".into());
    };
    let cipher = cipher.ok_or("暗号化の鍵（INTEGRATION_KEYS_FILE）が設定されていないため、画面の設定を読めません")?;
    let plain = cipher.open(kid, nonce, enc)?;
    serde_json::from_slice(&plain).map_err(|_| "秘密の設定の形が正しくありません".to_string())
}

fn str_field<'a>(v: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(|x| x.as_str()).map(str::trim).filter(|s| !s.is_empty())
}

fn build_notify_row(row: &TargetRow, cipher: Option<&secret::SecretCipher>) -> Result<NotifyTarget, String> {
    if row.provider != "gchat" {
        return Err(format!("知らないつなぎ先です: {}", row.provider));
    }
    let s = open_secret(row, cipher)?;
    let url = str_field(&s, "webhook_url").ok_or("Webhook の URL がありません")?;
    let notifier = GChatNotifier::new(url).map_err(|e| e.to_string())?;
    Ok(NotifyTarget {
        id: Some(row.id),
        label: row.label.clone(),
        provider: "gchat",
        source: Source::Db,
        destination: chat_destination(url),
        destination_name: str_field(&row.config, "destination_name").map(str::to_string),
        notifier: Arc::new(notifier),
    })
}

fn build_store_row(row: &TargetRow, cipher: Option<&secret::SecretCipher>) -> Result<StoreTarget, String> {
    if row.provider != "gdrive" {
        return Err(format!("知らないつなぎ先です: {}", row.provider));
    }
    let root = str_field(&row.config, "root_folder_id").ok_or("保存先のフォルダ ID がありません")?.to_string();
    let impersonate = str_field(&row.config, "impersonate").map(str::to_string);
    let s = open_secret(row, cipher)?;
    let key_json = str_field(&s, "service_account_json").ok_or("サービスアカウントの鍵がありません")?;
    let tokens = ServiceAccountTokenSource::from_json(key_json, impersonate).map_err(|e| e.to_string())?;
    let store = GDriveStore::new(GDriveConfig::new(root.clone()), Arc::new(tokens)).map_err(|e| e.to_string())?;
    Ok(StoreTarget {
        id: Some(row.id),
        label: row.label.clone(),
        provider: "gdrive",
        source: Source::Db,
        destination: root.clone(),
        destination_name: str_field(&row.config, "destination_name").map(str::to_string),
        store: Arc::new(store),
    })
}

fn notify_from_env() -> Result<NotifyTarget, String> {
    let url = env_nonempty("GOOGLE_CHAT_WEBHOOK_URL").ok_or("GOOGLE_CHAT_WEBHOOK_URL が設定されていません")?;
    let notifier = GChatNotifier::new(url.clone()).map_err(|e| e.to_string())?;
    Ok(NotifyTarget {
        id: None,
        label: "サーバの設定ファイル".to_string(),
        provider: "gchat",
        source: Source::Env,
        destination: chat_destination(&url),
        destination_name: None,
        notifier: Arc::new(notifier),
    })
}

fn store_from_env() -> Result<StoreTarget, String> {
    let root = env_nonempty("GOOGLE_DRIVE_ROOT_FOLDER_ID").ok_or("GOOGLE_DRIVE_ROOT_FOLDER_ID が設定されていません")?;
    let cred = env_nonempty("GOOGLE_DRIVE_CREDENTIALS_FILE").ok_or("GOOGLE_DRIVE_CREDENTIALS_FILE が設定されていません")?;
    let path = std::path::Path::new(&cred);
    if !path.exists() {
        return Err("GOOGLE_DRIVE_CREDENTIALS_FILE のファイルがありません".into());
    }
    let tokens = ServiceAccountTokenSource::from_file(path, env_nonempty("GOOGLE_DRIVE_IMPERSONATE_EMAIL")).map_err(|e| e.to_string())?;
    let store = GDriveStore::new(GDriveConfig::new(root.clone()), Arc::new(tokens)).map_err(|e| e.to_string())?;
    Ok(StoreTarget {
        id: None,
        label: "サーバの設定ファイル".to_string(),
        provider: "gdrive",
        source: Source::Env,
        destination: root.clone(),
        destination_name: None,
        store: Arc::new(store),
    })
}

impl Registry {
    fn build(rows: &[TargetRow], cipher: Option<&secret::SecretCipher>, pool: Option<PgPool>) -> Self {
        let manifest = load_manifest();

        // 通知: 画面の設定(有効な行)が1つでもあれば、それだけを使う。無ければサーバの設定ファイル
        let mut notify = Vec::new();
        let mut notify_broken = Vec::new();
        let notify_rows: Vec<&TargetRow> = rows.iter().filter(|r| r.use_id == NOTIFY_USE).collect();
        let mut notify_missing = String::new();
        if notify_rows.is_empty() {
            match notify_from_env() {
                Ok(t) => notify.push(t),
                Err(e) => notify_missing = e,
            }
        } else {
            for r in notify_rows {
                match build_notify_row(r, cipher) {
                    Ok(t) => notify.push(t),
                    Err(reason) => notify_broken.push(Broken { id: Some(r.id), label: r.label.clone(), reason }),
                }
            }
            if notify.is_empty() {
                notify_missing = "画面で設定した送り先を、どれも使えません".to_string();
            }
        }
        let composite = if notify.is_empty() {
            None
        } else {
            let mut c = CompositeNotifier::new();
            for t in &notify {
                c = c.with(t.label.clone(), t.notifier.clone() as Arc<dyn Notifier>);
            }
            Some(c)
        };

        // 保存先: 画面の設定(有効な行の最初の1つ)。無ければサーバの設定ファイル
        let mut store_missing = String::new();
        let mut store_broken = None;
        let store = match rows.iter().find(|r| r.use_id == STORE_USE) {
            Some(r) => match build_store_row(r, cipher) {
                Ok(t) => Some(t),
                Err(reason) => {
                    store_missing = format!("画面で設定した保存先を使えません: {reason}");
                    store_broken = Some(Broken { id: Some(r.id), label: r.label.clone(), reason });
                    None
                }
            },
            None => match store_from_env() {
                Ok(t) => Some(t),
                Err(e) => {
                    store_missing = e;
                    None
                }
            },
        };

        Self { manifest, notify, notify_broken, notify_missing, composite, store, store_broken, store_missing, pool }
    }

    pub fn ops_alert(&self) -> Result<&CompositeNotifier, &str> {
        self.composite.as_ref().ok_or(self.notify_missing.as_str())
    }

    pub fn file_store(&self) -> Result<Arc<GDriveStore>, &str> {
        self.store.as_ref().map(|s| s.store.clone()).ok_or(self.store_missing.as_str())
    }
}

/// 保存先（未設定なら理由を返す）。受信メールの添付の保存（received_doc_archive）
pub fn file_store() -> Result<Arc<dyn FileStore>, String> {
    registry().file_store().map(|s| s as Arc<dyn FileStore>).map_err(|e| e.to_string())
}

/// 発行した PDF の保存先（issued_pdf_archive）。受信メールの添付と同じ設定を使う
pub fn issued_pdf_store() -> Result<Arc<dyn FileStore>, String> {
    file_store()
}

/// 通知の頭に付ける「どのシステムか」の目印（開発標準書 §3.7）。
/// 送り先は全システム共通の2か所（本番「運用監視」/ステージング「運用監視（STG）」）で、
/// 同じスペースに複数のシステムの通知が並ぶため、システム名で見分ける。環境はスペースで分かるので付けない。
pub fn source_tag() -> &'static str {
    "【Sophia】"
}

/// 点検の結果の控え（サーバの設定ファイルから読んだ送り先は DB に行が無いので、ここに持つ。全体の結果もここ）
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub at: DateTime<Utc>,
    pub ok: bool,
    pub detail: Vec<String>,
}

static SNAPSHOTS: Mutex<Option<HashMap<String, Snapshot>>> = Mutex::new(None);

/// 点検のときに Drive から取ったフォルダ名(キーはフォルダ ID)
static FOLDER_NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

pub fn put_folder_name(folder_id: &str, name: String) {
    if let Ok(mut g) = FOLDER_NAMES.lock() {
        g.get_or_insert_with(HashMap::new).insert(folder_id.to_string(), name);
    }
}

pub fn get_folder_name(folder_id: &str) -> Option<String> {
    FOLDER_NAMES.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(folder_id).cloned()))
}

pub fn snapshot_key(use_id: &str, label: &str) -> String {
    format!("{use_id}\u{1f}{label}")
}

pub fn put_snapshot(key: String, snap: Snapshot) {
    if let Ok(mut g) = SNAPSHOTS.lock() {
        g.get_or_insert_with(HashMap::new).insert(key, snap);
    }
}

pub fn get_snapshot(key: &str) -> Option<Snapshot> {
    SNAPSHOTS.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(key).cloned()))
}

/// ローカル開発では、運用の Chat へ実際に送らない。
/// 理由: 手元の開発サーバ(SMTP 未設定の DB)が取込失敗を検知し、運用の Chat に本物の障害として
/// 投稿された(2026-10-13 18:30)。`ENV_NAME` 未設定もローカル扱い。通知の確認が目的のときだけ
/// `OPS_NOTIFY_FROM_LOCAL=1` で送れる。
fn local_must_not_notify(env_name: Option<&str>, opt_in: Option<&str>) -> bool {
    let is_local = matches!(env_name.map(str::trim), None | Some("") | Some("local"));
    is_local && opt_in.map(str::trim) != Some("1")
}

/// 異常の通知（ops_alert）へ送る。送り先ごとに失敗を記録し、1つも届かなければ error ログを残す。
/// 送り先が未設定なら warn ログを残す（以前は debug で、止まっていても気づけなかった。DEMO-000207）。
/// 画面で設定した送り先は、届いたかどうかを DB に記録する（毎朝の点検で見る。設計書 2.3）。
pub async fn notify_ops(notice: Notice) {
    if local_must_not_notify(
        std::env::var("ENV_NAME").ok().as_deref(),
        std::env::var("OPS_NOTIFY_FROM_LOCAL").ok().as_deref(),
    ) {
        tracing::warn!(
            "[外部連携] ローカル開発(ENV_NAME=local)のため、異常の通知は実際には送りません（運用の Chat への誤投稿防止）: {}",
            notice.to_plain_text().lines().nth(1).unwrap_or("")
        );
        return;
    }
    let reg = registry();
    let notifier = match reg.ops_alert() {
        Ok(n) => n,
        Err(why) => {
            tracing::warn!("[外部連携] 異常の通知（ops_alert）を送れません（{why}）。設定 > 外部連携 で送り先を設定してください");
            return;
        }
    };
    let mut delivered = 0usize;
    for d in notifier.notify_all(&notice).await {
        let (ok, detail) = match &d.result {
            Ok(()) => {
                delivered += 1;
                tracing::info!("[外部連携] 通知を送りました（{}）", d.label);
                (true, "届いた".to_string())
            }
            Err(e) => {
                tracing::error!(
                    "{}",
                    ops_message::fail("外部連携", &d.label, "異常の通知", "障害の通知が届いていない可能性", e)
                );
                (false, e.to_string())
            }
        };
        let target_id = reg.notify.iter().find(|t| t.label == d.label).and_then(|t| t.id);
        if let (Some(id), Some(pool)) = (target_id, reg.pool.as_ref()) {
            if let Err(e) = integration_repo::record_delivery(pool, id, ok, &detail).await {
                tracing::warn!("[外部連携] 送信の結果を記録できません（{}）: {e}", d.label);
            }
        }
    }
    if delivered == 0 {
        tracing::error!("[外部連携] 異常の通知が、どの送り先にも届きませんでした");
    }
}

/// 画面の「テスト送信」。送り先1つに、テストの通知を1件送る（相手に書き込む）。
/// `target_id` が None ならサーバの設定ファイルの送り先。結果を返し、画面の設定の送り先なら DB に記録する。
pub async fn send_test(pool: &PgPool, target_id: Option<i64>, by: i64) -> Result<(), String> {
    let reg = registry();
    let t = reg
        .notify
        .iter()
        .find(|t| t.id == target_id)
        .ok_or("その送り先は、今は使える状態ではありません（点検の結果を確かめてください）")?;
    let result = t.notifier.send_test("Sophia の設定 > 外部連携").await.map_err(|e| e.to_string());
    if let Some(id) = t.id {
        let (ok, detail) = match &result {
            Ok(()) => (true, "届いた".to_string()),
            Err(e) => (false, e.clone()),
        };
        if let Err(e) = integration_repo::record_test(pool, id, ok, by, &detail).await {
            tracing::warn!("[外部連携] テスト送信の結果を記録できません: {e}");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_does_not_notify_unless_opted_in() {
        assert!(local_must_not_notify(Some("local"), None));
        assert!(local_must_not_notify(None, None));
        assert!(local_must_not_notify(Some(""), Some("0")));
        assert!(!local_must_not_notify(Some("local"), Some("1")));
        assert!(!local_must_not_notify(Some("staging"), None));
        assert!(!local_must_not_notify(Some("production"), None));
    }

    use super::*;

    #[test]
    fn embedded_manifest_parses_and_has_core_uses() {
        let m = Manifest::from_toml(MANIFEST_TOML).expect("integrations.toml が読めること");
        for id in ["ops_alert", "issued_pdf_archive", "received_doc_archive", "mail_send", "db_backup"] {
            assert!(m.get(id).is_some(), "{id} が一覧にあること");
        }
        assert_eq!(m.get("ops_alert").and_then(|u| u.fallback.as_deref()), Some("mail_admin"));
    }

    #[test]
    fn every_use_is_handled_by_check() {
        // 一覧に用途を足したら、check.rs の調べ方も足すこと（調べ忘れは「未設定」扱いになる）
        let m = Manifest::from_toml(MANIFEST_TOML).expect("integrations.toml が読めること");
        for u in &m.uses {
            assert!(check::HANDLED_USES.contains(&u.id.as_str()), "{} の調べ方が check.rs にありません", u.id);
        }
    }

    #[test]
    fn chat_destination_hides_key() {
        let d = chat_destination("https://chat.googleapis.com/v1/spaces/AAQA5whOuNU/messages?key=SECRET&token=SECRET");
        assert_eq!(d, "AAQA5whOuNU");
        assert!(!d.contains("SECRET"));
    }

    fn row(id: i64, use_id: &str, provider: &str) -> TargetRow {
        TargetRow {
            id,
            use_id: use_id.into(),
            label: format!("行{id}"),
            provider: provider.into(),
            config: serde_json::json!({ "root_folder_id": "ROOT" }),
            secret_enc: Some(vec![1]),
            secret_nonce: Some(vec![0; 12]),
            secret_key_id: Some("k1".into()),
            enabled: true,
        }
    }

    #[test]
    fn rows_without_cipher_are_broken_not_silently_env() {
        // 画面の設定があるのに鍵が無いときは、サーバの設定ファイルに黙って戻らず、「壊れた送り先」として点検に出す
        let r = Registry::build(&[row(1, NOTIFY_USE, "gchat"), row(2, STORE_USE, "gdrive")], None, None);
        assert!(r.notify.is_empty());
        assert_eq!(r.notify_broken.len(), 1);
        assert!(r.notify_broken[0].reason.contains("INTEGRATION_KEYS_FILE"));
        assert!(r.ops_alert().is_err());
        assert!(r.store.is_none());
        assert!(r.store_broken.is_some());
        assert!(r.file_store().is_err());
    }

    #[test]
    fn unknown_provider_is_broken() {
        let r = Registry::build(&[row(1, NOTIFY_USE, "fax")], None, None);
        assert!(r.notify_broken[0].reason.contains("知らないつなぎ先"));
    }
}
