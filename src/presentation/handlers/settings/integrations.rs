/// presentation/handlers/settings/integrations.rs — 外部連携の設定画面の API（Admin専用。DEMO-000215）
///
/// 画面: 外部連携(サイドバー「設定」> タブ「外部連携」)。設計書 3.2、承認済みのモック(2026-10-12)。
/// - 秘密の値(Webhook の URL・サービスアカウントの鍵)は、どの応答にも出さない(`has_secret` だけ)。
/// - 変えたら、変更の記録を残し(秘密の値は書かない)、部品を組み立て直して、その用途を点検する。

use axum::{extract::{Path, State}, http::StatusCode, Extension, Json};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::infrastructure::integrations::{self as integ, check, secret::SecretCipher, Source, NOTIFY_USE, STORE_USE};
use crate::infrastructure::repositories::{integration_repo, issued_document_repo, mail_repo};
use crate::presentation::middleware::role::AuthUser;

type HandlerResult = Result<Json<Value>, (StatusCode, Json<Value>)>;

fn bad(msg: impl Into<String>) -> (StatusCode, Json<Value>) {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg.into() })))
}

fn internal(context: &str, e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    tracing::error!("[外部連携の設定] {context}: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "サーバーエラーが発生しました。しばらく経ってから再度お試しください。" })))
}

/// 日本時間で表示する(DB は世界標準時)
fn jst(t: chrono::DateTime<chrono::Utc>) -> String {
    match chrono::FixedOffset::east_opt(9 * 3600) {
        Some(tz) => t.with_timezone(&tz).format("%Y-%m-%d %H:%M").to_string(),
        None => t.format("%Y-%m-%d %H:%M(UTC)").to_string(),
    }
}

fn provider_label(p: &str) -> &'static str {
    match p {
        "gchat" => "Google Chat",
        "gdrive" => "Google Drive",
        _ => "(不明)",
    }
}

/// 画面に出す状態(秘密の値は含めない)
async fn build_state(pool: &PgPool) -> Result<Value, String> {
    let reg = integ::registry();
    let views = integration_repo::list_views(pool).await.map_err(|e| e.to_string())?;
    let find_view = |id: Option<i64>| id.and_then(|i| views.iter().find(|v| v.id == i));

    let target_json = |use_id: &str, id: Option<i64>, label: &str, provider: &str, source: Source, destination: &str, destination_name: Option<String>, usable: bool, reason: Option<&str>| {
        let v = find_view(id);
        let (check_at, check_ok, check_detail) = match v {
            Some(v) => (v.last_check_at.map(jst), v.last_check_ok, v.last_check_detail.clone()),
            None => match integ::get_snapshot(&integ::snapshot_key(use_id, label)) {
                Some(s) => (Some(jst(s.at)), Some(s.ok), Some(json!(s.detail))),
                None => (None, None, None),
            },
        };
        json!({
            "id": id,
            "label": label,
            "provider": provider,
            "provider_label": provider_label(provider),
            "source": if source == Source::Db { "db" } else { "env" },
            // 宛先は名前で出す(無ければ None。画面で「未登録」と表示する)。ID は小さく添える用
            "destination": destination,
            "destination_name": destination_name,
            "usable": usable,
            "problem": reason,
            "config": v.map(|v| v.config.clone()).unwrap_or(json!({})),
            "has_secret": v.map(|v| v.has_secret).unwrap_or(source == Source::Env),
            "check": { "at": check_at, "ok": check_ok, "detail": check_detail },
            "test": { "at": v.and_then(|v| v.last_test_at.map(jst)), "ok": v.and_then(|v| v.last_test_ok), "by": v.and_then(|v| v.last_test_by_name.clone()), "detail": v.and_then(|v| v.last_test_detail.clone()) },
            "delivery": { "at": v.and_then(|v| v.last_delivery_at.map(jst)), "ok": v.and_then(|v| v.last_delivery_ok), "detail": v.and_then(|v| v.last_delivery_detail.clone()) },
        })
    };

    let mut notify_targets: Vec<Value> = reg
        .notify
        .iter()
        .map(|t| target_json(NOTIFY_USE, t.id, &t.label, t.provider, t.source, &t.destination, t.destination_name.clone(), true, None))
        .collect();
    for b in &reg.notify_broken {
        let provider = find_view(b.id).map(|v| v.provider.clone()).unwrap_or_default();
        let name = find_view(b.id).and_then(|v| v.config.get("destination_name").and_then(|x| x.as_str()).map(str::to_string));
        notify_targets.push(target_json(NOTIFY_USE, b.id, &b.label, &provider, Source::Db, "", name, false, Some(&b.reason)));
    }
    let mut store_targets: Vec<Value> = Vec::new();
    if let Some(t) = &reg.store {
        let name = t.destination_name.clone().or_else(|| integ::get_folder_name(&t.destination));
        store_targets.push(target_json(STORE_USE, t.id, &t.label, t.provider, t.source, &t.destination, name, true, None));
    }
    if let Some(b) = &reg.store_broken {
        let provider = find_view(b.id).map(|v| v.provider.clone()).unwrap_or_default();
        store_targets.push(target_json(STORE_USE, b.id, &b.label, &provider, Source::Db, "", None, false, Some(&b.reason)));
    }

    let overall = integ::get_snapshot(check::OVERALL_KEY);
    let pending = issued_document_repo::pending_count(pool).await.unwrap_or(0);
    let last_stored = issued_document_repo::last_stored(pool).await.ok().flatten();
    let changes = integration_repo::list_changes(pool, 20).await.map_err(|e| e.to_string())?;
    let from = crate::domain::services::email_service::get_notify_email(pool).await;
    let last_import = mail_repo::latest_sync_checkpoint(pool).await.ok().flatten();

    Ok(json!({
        "overall": overall.map(|s| json!({ "at": jst(s.at), "ok": s.ok, "problems": s.detail })),
        "cipher_ready": SecretCipher::from_env().is_ok(),
        "notify": { "use_id": NOTIFY_USE, "targets": notify_targets },
        "store": {
            "use_id": STORE_USE,
            "targets": store_targets,
            "pending_count": pending,
            "last_stored": last_stored.map(|(name, at)| json!({ "name": name, "at": jst(at) })),
        },
        "mail": { "from": from, "last_import_at": last_import.map(jst) },
        "changes": changes.iter().map(|c| json!({
            "at": jst(c.changed_at), "by": c.changed_by_name, "use_id": c.use_id, "summary": c.summary,
        })).collect::<Vec<_>>(),
    }))
}

/// GET /api/v1/settings/integrations — 画面の状態
pub async fn get_state(State(pool): State<PgPool>) -> HandlerResult {
    build_state(&pool).await.map(Json).map_err(|e| internal("状態の取得", e))
}

/// GET /api/v1/settings/integrations/alert — 画面の上部の赤い帯の元(管理者のみ)
/// 点検の結果のうち、いちばん新しいもの。問題が無ければ `problems` は空(帯は出ない)。
pub async fn get_alert(State(pool): State<PgPool>) -> HandlerResult {
    let st = check::current_alert(&pool).await;
    Ok(Json(match st {
        Some(a) => json!({ "checked_at": jst(a.at), "problems": a.problems }),
        None => json!({ "checked_at": null, "problems": [] }),
    }))
}

/// POST /api/v1/settings/integrations/check — 全部を点検する(相手に書き込まない)
pub async fn run_check(State(pool): State<PgPool>) -> HandlerResult {
    check::run(&pool, true).await;
    build_state(&pool).await.map(Json).map_err(|e| internal("状態の取得", e))
}

#[derive(Debug, Deserialize)]
pub struct TestReq {
    /// null ならサーバの設定ファイルの送り先
    pub target_id: Option<i64>,
}

/// POST /api/v1/settings/integrations/test — 通知の送り先に、テストの通知を1件送る
pub async fn send_test(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Json(req): Json<TestReq>) -> HandlerResult {
    let result = integ::send_test(&pool, req.target_id, auth.user.id).await;
    let state = build_state(&pool).await.map_err(|e| internal("状態の取得", e))?;
    Ok(Json(json!({ "ok": result.is_ok(), "error": result.err(), "state": state })))
}

#[derive(Debug, Deserialize)]
pub struct TargetReq {
    /// 足すときだけ: 'ops_alert' か 'issued_pdf_archive'
    #[serde(default)]
    pub use_id: Option<String>,
    pub label: String,
    pub provider: String,
    /// Google Chat: なし / Google Drive: root_folder_id, impersonate
    #[serde(default)]
    pub config: Value,
    /// 秘密の設定。変えないときは null(入れ直すときだけ送る)。
    /// Google Chat: { "webhook_url": "…" } / Google Drive: { "service_account_json": "…" }
    #[serde(default)]
    pub secret: Option<Value>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// 入力を確かめ、秘密でない設定を整える
fn validate(use_id: &str, req: &TargetReq, is_new: bool) -> Result<Value, String> {
    let label = req.label.trim();
    if label.is_empty() || label.chars().count() > 60 {
        return Err("名前は1〜60文字で入れてください".into());
    }
    match (use_id, req.provider.as_str()) {
        (NOTIFY_USE, "gchat") => {
            let dest = req.config.get("destination_name").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if dest.is_empty() || dest.chars().count() > 60 {
                return Err("宛先の名前(Chat のスペース名)を1〜60文字で入れてください".into());
            }
            if let Some(s) = &req.secret {
                let url = s.get("webhook_url").and_then(|v| v.as_str()).unwrap_or("").trim();
                if !url.starts_with("https://chat.googleapis.com/v1/spaces/") || !url.contains("key=") || !url.contains("token=") {
                    return Err("Google Chat の Incoming Webhook の URL(https://chat.googleapis.com/v1/spaces/…?key=…&token=…)を入れてください".into());
                }
            } else if is_new {
                return Err("Webhook の URL を入れてください".into());
            }
            Ok(json!({ "destination_name": dest }))
        }
        (STORE_USE, "gdrive") => {
            let root = req.config.get("root_folder_id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if root.is_empty() || !root.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err("保存先のフォルダ ID を入れてください(フォルダを開いたときのアドレス欄の …/folders/ の後ろ)".into());
            }
            let impersonate = req.config.get("impersonate").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if let Some(s) = &req.secret {
                let key = s.get("service_account_json").and_then(|v| v.as_str()).unwrap_or("");
                let parsed: Value = serde_json::from_str(key).map_err(|_| "サービスアカウントの鍵(JSON)の形が正しくありません".to_string())?;
                if parsed.get("type").and_then(|v| v.as_str()) != Some("service_account") || parsed.get("private_key").is_none() {
                    return Err("サービスアカウントの鍵(JSON)ではありません".into());
                }
            } else if is_new {
                return Err("サービスアカウントの鍵(JSON)を選んでください".into());
            }
            Ok(json!({ "root_folder_id": root, "impersonate": impersonate }))
        }
        (NOTIFY_USE, _) | (STORE_USE, _) => Err("このつなぎ先は、まだ使えません(準備中)".into()),
        _ => Err("知らない用途です".into()),
    }
}

/// 秘密の設定を暗号化する
fn seal(secret: &Option<Value>) -> Result<Option<crate::infrastructure::integrations::secret::Sealed>, String> {
    match secret {
        None => Ok(None),
        Some(v) => {
            let cipher = SecretCipher::from_env()?;
            let plain = serde_json::to_vec(v).map_err(|e| e.to_string())?;
            cipher.seal(&plain).map(Some)
        }
    }
}

/// POST /api/v1/settings/integrations/targets — 送り先・保存先を足す
pub async fn add_target(State(pool): State<PgPool>, Extension(auth): Extension<AuthUser>, Json(req): Json<TargetReq>) -> HandlerResult {
    let use_id = req.use_id.clone().unwrap_or_default();
    let config = validate(&use_id, &req, true).map_err(bad)?;
    if use_id == STORE_USE {
        let rows = integration_repo::list_all_rows(&pool).await.map_err(|e| internal("設定の読み込み", e))?;
        if rows.iter().any(|r| r.use_id == STORE_USE) {
            return Err(bad("保存先は1つだけです。今の保存先を「変更」してください"));
        }
    }
    let sealed = seal(&req.secret).map_err(bad)?;
    let s = sealed.as_ref().map(|s| integration_repo::SealedSecret { key_id: &s.key_id, nonce: &s.nonce, ciphertext: &s.ciphertext });
    let id = integration_repo::insert_target(&pool, &use_id, req.label.trim(), &req.provider, &config, s, auth.user.id)
        .await
        .map_err(|e| {
            if e.to_string().contains("uq_integration_target_label") {
                bad("同じ名前の送り先があります")
            } else {
                internal("送り先の追加", e)
            }
        })?;
    let summary = format!("「{}」を足した({})", req.label.trim(), provider_label(&req.provider));
    integration_repo::insert_change(&pool, auth.user.id, &use_id, Some(id), &summary).await.map_err(|e| internal("変更の記録", e))?;
    integ::reload(&pool).await;
    check::run(&pool, true).await;
    build_state(&pool).await.map(Json).map_err(|e| internal("状態の取得", e))
}

/// PUT /api/v1/settings/integrations/targets/{id} — 送り先・保存先を変える(秘密は入れ直すときだけ)
pub async fn update_target(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthUser>,
    Path(id): Path<i64>,
    Json(req): Json<TargetReq>,
) -> HandlerResult {
    let row = integration_repo::find_row(&pool, id)
        .await
        .map_err(|e| internal("設定の読み込み", e))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "送り先が見つかりません" }))))?;
    if row.provider != req.provider {
        return Err(bad("つなぎ先の種類は変えられません。新しく足してください"));
    }
    let config = validate(&row.use_id, &req, false).map_err(bad)?;
    let sealed = seal(&req.secret).map_err(bad)?;
    let s = sealed.as_ref().map(|s| integration_repo::SealedSecret { key_id: &s.key_id, nonce: &s.nonce, ciphertext: &s.ciphertext });
    integration_repo::update_target(&pool, id, req.label.trim(), &config, req.enabled, s, auth.user.id)
        .await
        .map_err(|e| internal("送り先の変更", e))?;
    let mut parts = Vec::new();
    if row.label != req.label.trim() {
        parts.push(format!("名前を「{}」にした", req.label.trim()));
    }
    if row.config != config {
        parts.push("設定を変えた".to_string());
    }
    if req.secret.is_some() {
        parts.push(if row.provider == "gchat" { "Webhook を入れ直した".to_string() } else { "鍵を入れ直した".to_string() });
    }
    if row.enabled != req.enabled {
        parts.push(if req.enabled { "有効にした".to_string() } else { "無効にした".to_string() });
    }
    let summary = format!("「{}」: {}", row.label, if parts.is_empty() { "変更なし".to_string() } else { parts.join("、") });
    integration_repo::insert_change(&pool, auth.user.id, &row.use_id, Some(id), &summary).await.map_err(|e| internal("変更の記録", e))?;
    integ::reload(&pool).await;
    check::run(&pool, true).await;
    build_state(&pool).await.map(Json).map_err(|e| internal("状態の取得", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(provider: &str, label: &str, config: Value, secret: Option<Value>) -> TargetReq {
        TargetReq { use_id: None, label: label.into(), provider: provider.into(), config, secret, enabled: true }
    }

    #[test]
    fn validates_chat_url_and_requires_it_when_new() {
        let ok = req("gchat", "運用監視", json!({ "destination_name": "運用監視（STG）" }), Some(json!({ "webhook_url": "https://chat.googleapis.com/v1/spaces/A/messages?key=k&token=t" })));
        assert!(validate(NOTIFY_USE, &ok, true).is_ok());
        let bad_url = req("gchat", "x", json!({ "destination_name": "運用監視" }), Some(json!({ "webhook_url": "https://hooks.slack.com/x" })));
        assert!(validate(NOTIFY_USE, &bad_url, true).is_err());
        let none = req("gchat", "x", json!({ "destination_name": "運用監視" }), None);
        assert!(validate(NOTIFY_USE, &none, true).is_err(), "足すときは必須");
        assert!(validate(NOTIFY_USE, &none, false).is_ok(), "変えるときは入れ直さなくてよい");
        let no_name = req("gchat", "x", json!({}), None);
        assert!(validate(NOTIFY_USE, &no_name, false).unwrap_err().contains("スペース名"), "宛先の名前は必須");
    }

    #[test]
    fn validates_drive_folder_and_key() {
        let key = json!({ "service_account_json": "{\"type\":\"service_account\",\"private_key\":\"x\",\"client_email\":\"a@b\",\"token_uri\":\"t\"}" });
        let ok = req("gdrive", "Drive", json!({ "root_folder_id": "1bHlH4eiCo4QJS0fhfujh0slGK2xmDqnJ" }), Some(key.clone()));
        assert_eq!(validate(STORE_USE, &ok, true).unwrap()["root_folder_id"], "1bHlH4eiCo4QJS0fhfujh0slGK2xmDqnJ");
        let bad_folder = req("gdrive", "Drive", json!({ "root_folder_id": "a/b" }), Some(key));
        assert!(validate(STORE_USE, &bad_folder, true).is_err());
        let bad_key = req("gdrive", "Drive", json!({ "root_folder_id": "ROOT" }), Some(json!({ "service_account_json": "{}" })));
        assert!(validate(STORE_USE, &bad_key, true).is_err());
    }

    #[test]
    fn rejects_not_ready_providers_and_bad_labels() {
        assert!(validate(NOTIFY_USE, &req("slack", "x", json!({}), None), true).unwrap_err().contains("準備中"));
        assert!(validate(NOTIFY_USE, &req("gchat", "  ", json!({}), None), false).is_err());
        assert!(validate("mail_send", &req("gchat", "x", json!({}), None), false).is_err());
    }

    #[test]
    fn jst_is_plus_nine() {
        let t = chrono::DateTime::parse_from_rfc3339("2026-10-11T14:10:00Z").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(jst(t), "2026-10-11 23:10");
    }
}
