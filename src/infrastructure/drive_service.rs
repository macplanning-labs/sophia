/// infrastructure/drive_service.rs — Google Drive連携サービス
///
/// Django版 core/domain/services/drive_service.py (362行) から移植。
/// サービスアカウント認証でGoogle Driveにドキュメントをアップロードする。
///
/// フォルダ構成:
///     Sophia (ルート)
///     ├── パートナー管理/
///     │   └── {会社名}/
///     │       ├── 契約書/
///     │       ├── 注文書/
///     │       └── 稼働報告書/
///     └── クライアント管理/
///         └── {会社名}/
///             ├── 注文書/
///             ├── 稼働報告書/
///             └── 請求書/

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

// ── ドキュメント種別ラベル ──

fn doc_type_label(doc_type: &str) -> &str {
    match doc_type {
        "contract" => "契約書",
        "order" => "注文書",
        "work_report" => "稼働報告書",
        "invoice" => "請求書",
        "payment" => "支払通知書",
        _ => doc_type,
    }
}

// ============================================================
// Google Service Account JWT 認証
// ============================================================

#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
    sub: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

/// サービスアカウントキーからアクセストークンを取得する
async fn get_access_token(key: &ServiceAccountKey, impersonate: Option<&str>, scope: &str) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = JwtClaims {
        iss: key.client_email.clone(),
        scope: scope.to_string(),
        aud: key.token_uri.clone(),
        exp: now + 3600,
        iat: now,
        sub: impersonate.map(|s| s.to_string()),
    };

    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let encoding_key = jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())
        .context("RSA秘密鍵の解析に失敗")?;
    let jwt = jsonwebtoken::encode(&header, &claims, &encoding_key)
        .context("JWT生成に失敗")?;

    let client = reqwest::Client::new();
    let resp = client.post(&key.token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ])
        .send()
        .await
        .context("トークンリクエストに失敗")?;

    let token: TokenResponse = resp.json().await.context("トークンレスポンスの解析に失敗")?;
    Ok(token.access_token)
}

/// サービスアカウント設定（`GOOGLE_DRIVE_CREDENTIALS_FILE`）から指定スコープのアクセストークンを取得する。
/// 未設定ならNone（呼び出し元は機能をスキップする）。`sheets_service.rs`など他のGoogle API連携でも
/// 同じサービスアカウントを使い回すための共通ヘルパー。
pub(crate) async fn get_scoped_access_token(scope: &str) -> Result<Option<String>> {
    let credentials_file = std::env::var("GOOGLE_DRIVE_CREDENTIALS_FILE").unwrap_or_default();
    if credentials_file.is_empty() || !std::path::Path::new(&credentials_file).exists() {
        info!("[Google API] サービスアカウントキーが未設定のためスキップ");
        return Ok(None);
    }

    let key_json = std::fs::read_to_string(&credentials_file)
        .context("サービスアカウントキーの読み込みに失敗")?;
    let key: ServiceAccountKey = serde_json::from_str(&key_json)
        .context("サービスアカウントキーの解析に失敗")?;

    let impersonate = std::env::var("GOOGLE_DRIVE_IMPERSONATE_EMAIL").ok();
    let token = get_access_token(&key, impersonate.as_deref(), scope).await?;
    Ok(Some(token))
}

// ============================================================
// Drive API ヘルパー
// ============================================================

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
    #[serde(rename = "webViewLink", default)]
    web_view_link: String,
}

#[derive(Debug, Deserialize)]
struct DriveFileList {
    files: Vec<DriveFile>,
}

/// フォルダを検索し、なければ作成する
async fn find_or_create_folder(
    client: &reqwest::Client,
    token: &str,
    folder_name: &str,
    parent_id: &str,
) -> Result<String> {
    let query = format!(
        "name='{}' and '{}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false",
        folder_name, parent_id
    );

    let resp: DriveFileList = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(token)
        .query(&[
            ("q", query.as_str()),
            ("spaces", "drive"),
            ("fields", "files(id,name)"),
            ("pageSize", "1"),
            ("supportsAllDrives", "true"),
            ("includeItemsFromAllDrives", "true"),
        ])
        .send().await?
        .json().await?;

    if let Some(f) = resp.files.first() {
        return Ok(f.id.clone());
    }

    // フォルダ作成
    let metadata = serde_json::json!({
        "name": folder_name,
        "mimeType": "application/vnd.google-apps.folder",
        "parents": [parent_id],
    });

    let created: DriveFile = client
        .post("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(token)
        .query(&[("supportsAllDrives", "true"), ("fields", "id")])
        .json(&metadata)
        .send().await?
        .json().await?;

    info!("[Google Drive] フォルダ作成: {} (ID: {})", folder_name, created.id);
    Ok(created.id)
}

/// ファイルをアップロードする
async fn upload_file(
    client: &reqwest::Client,
    token: &str,
    content: &[u8],
    filename: &str,
    parent_id: &str,
    mimetype: &str,
) -> Result<(String, String)> {
    let metadata = serde_json::json!({
        "name": filename,
        "parents": [parent_id],
    });

    let form = reqwest::multipart::Form::new()
        .part("metadata", reqwest::multipart::Part::text(metadata.to_string())
            .mime_str("application/json")?)
        .part("file", reqwest::multipart::Part::bytes(content.to_vec())
            .file_name(filename.to_string())
            .mime_str(mimetype)?);

    let uploaded: DriveFile = client
        .post("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&supportsAllDrives=true&fields=id,webViewLink")
        .bearer_auth(token)
        .multipart(form)
        .send().await?
        .json().await?;

    let link = if uploaded.web_view_link.is_empty() {
        format!("https://drive.google.com/file/d/{}/view", uploaded.id)
    } else {
        uploaded.web_view_link
    };

    info!("[Google Drive] アップロード成功: {} (ID: {})", filename, uploaded.id);
    Ok((uploaded.id, link))
}

/// 既存ファイルを検索する
async fn find_existing_file(
    client: &reqwest::Client,
    token: &str,
    filename: &str,
    parent_id: &str,
) -> Result<Option<String>> {
    let query = format!(
        "name='{}' and '{}' in parents and trashed=false",
        filename, parent_id
    );

    let resp: DriveFileList = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(token)
        .query(&[
            ("q", query.as_str()),
            ("spaces", "drive"),
            ("fields", "files(id)"),
            ("pageSize", "1"),
            ("supportsAllDrives", "true"),
            ("includeItemsFromAllDrives", "true"),
        ])
        .send().await?
        .json().await?;

    Ok(resp.files.first().map(|f| f.id.clone()))
}

/// 既存ファイルを上書き更新する
async fn update_file(
    client: &reqwest::Client,
    token: &str,
    file_id: &str,
    content: &[u8],
    mimetype: &str,
) -> Result<(String, String)> {
    let _form = reqwest::multipart::Part::bytes(content.to_vec())
        .mime_str(mimetype)?;

    let updated: DriveFile = client
        .patch(&format!(
            "https://www.googleapis.com/upload/drive/v3/files/{}?uploadType=media&supportsAllDrives=true&fields=id,webViewLink",
            file_id
        ))
        .bearer_auth(token)
        .header("Content-Type", mimetype)
        .body(content.to_vec())
        .send().await?
        .json().await?;

    info!("[Google Drive] 更新成功: {}", file_id);
    let link = if updated.web_view_link.is_empty() {
        format!("https://drive.google.com/file/d/{}/view", updated.id)
    } else {
        updated.web_view_link
    };
    Ok((updated.id, link))
}

// ============================================================
// 統一ドキュメントアップロードAPI
// ============================================================

/// 統一ドキュメントアップロードAPI
///
/// management_type: "partner" or "client"
/// company_name: パートナー名 or クライアント名
/// doc_type: "contract", "order", "work_report", "invoice", "payment"
pub async fn upload_document(
    management_type: &str,
    company_name: &str,
    doc_type: &str,
    filename: &str,
    file_bytes: &[u8],
    mimetype: Option<&str>,
) -> Result<(String, String)> {
    let root_folder_id = std::env::var("GOOGLE_DRIVE_ROOT_FOLDER_ID").unwrap_or_default();
    if root_folder_id.is_empty() {
        info!("[Google Drive] ROOT_FOLDER_IDが未設定のためスキップ");
        return Ok((String::new(), String::new()));
    }

    let credentials_file = std::env::var("GOOGLE_DRIVE_CREDENTIALS_FILE").unwrap_or_default();
    if credentials_file.is_empty() || !std::path::Path::new(&credentials_file).exists() {
        info!("[Google Drive] サービスアカウントキーが未設定のためスキップ");
        return Ok((String::new(), String::new()));
    }

    let key_json = std::fs::read_to_string(&credentials_file)
        .context("サービスアカウントキーの読み込みに失敗")?;
    let key: ServiceAccountKey = serde_json::from_str(&key_json)
        .context("サービスアカウントキーの解析に失敗")?;

    let impersonate = std::env::var("GOOGLE_DRIVE_IMPERSONATE_EMAIL").ok();
    let token = get_access_token(&key, impersonate.as_deref(), "https://www.googleapis.com/auth/drive").await?;
    let client = reqwest::Client::new();

    // フォルダ階層を構築
    let mgmt_label = if management_type == "partner" {
        "パートナー管理"
    } else {
        "クライアント管理"
    };
    let mgmt_folder_id = find_or_create_folder(&client, &token, mgmt_label, &root_folder_id).await?;
    let company_folder_id = find_or_create_folder(&client, &token, company_name, &mgmt_folder_id).await?;
    let doc_label = doc_type_label(doc_type);
    let doc_folder_id = find_or_create_folder(&client, &token, doc_label, &company_folder_id).await?;

    // MIMEタイプ推定
    let mime = mimetype.unwrap_or_else(|| {
        if filename.ends_with(".xlsx") || filename.ends_with(".xlsm") {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        } else if filename.ends_with(".html") {
            "text/html"
        } else {
            "application/pdf"
        }
    });

    // 既存ファイルがあれば上書き
    if let Some(existing_id) = find_existing_file(&client, &token, filename, &doc_folder_id).await? {
        return update_file(&client, &token, &existing_id, file_bytes, mime).await;
    }

    upload_file(&client, &token, file_bytes, filename, &doc_folder_id, mime).await
}

// ============================================================
// 公開API — Sophia固有ラッパー
// ============================================================

/// 注文書PDFをGoogle Driveにアップロードする
pub async fn upload_order_pdf(partner_name: &str, order_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("partner", partner_name, "order", &format!("order_{}.pdf", order_id), pdf_bytes, None).await
}

/// 支払通知書PDFをGoogle Driveにアップロードする
pub async fn upload_payment_notice_pdf(partner_name: &str, notice_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("partner", partner_name, "payment", &format!("payment_{}.pdf", notice_id), pdf_bytes, None).await
}

/// 請求書PDFをGoogle Driveにアップロードする
pub async fn upload_invoice_pdf(client_name: &str, invoice_id: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("client", client_name, "invoice", &format!("invoice_{}.pdf", invoice_id), pdf_bytes, None).await
}

/// 稼働報告書をGoogle Driveにアップロードする
pub async fn upload_work_report(client_name: &str, filename: &str, file_bytes: &[u8]) -> Result<(String, String)> {
    upload_document("client", client_name, "work_report", filename, file_bytes, None).await
}

/// 契約書PDFをGoogle Driveにアップロードする
pub async fn upload_contract_pdf(partner_name: &str, signed_at: &str, pdf_bytes: &[u8]) -> Result<(String, String)> {
    let filename = format!("契約書_{}_{}.pdf", partner_name, signed_at);
    upload_document("partner", partner_name, "contract", &filename, pdf_bytes, None).await
}

/// DriveファイルIDからURLを生成する
pub fn get_drive_file_url(file_id: &str) -> String {
    if file_id.is_empty() {
        String::new()
    } else {
        format!("https://drive.google.com/file/d/{}/view", file_id)
    }
}
