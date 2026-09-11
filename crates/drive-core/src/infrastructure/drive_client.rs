//! Google Drive API クライアント実装
//!
//! サービスアカウント JWT 認証、フォルダ管理、ファイルアップロード機能を提供します。

use crate::error::{DriveError, Result};
use chrono::Utc;
use serde_json::json;
use tracing::info;

/// Google Drive クライアント
pub struct DriveClient {
    credentials_file: String,
    root_folder_id: String,
}

/// アップロード済みファイル情報
#[derive(Debug, Clone)]
pub struct UploadedFile {
    /// Google Drive のファイルID
    pub file_id: String,
    /// Google Drive のビューリンク
    pub web_view_link: String,
}

impl DriveClient {
    /// 環境変数からクライアントを構築する
    ///
    /// 以下の環境変数を読み取ります：
    /// - `GOOGLE_DRIVE_ROOT_FOLDER_ID`: ルートフォルダID
    /// - `GOOGLE_DRIVE_CREDENTIALS_FILE`: サービスアカウントキーファイルのパス
    ///
    /// 空またはファイルが存在しない場合は `None` を返します（スキップ扱い）
    pub fn from_env() -> Option<Self> {
        let root_folder_id = std::env::var("GOOGLE_DRIVE_ROOT_FOLDER_ID").ok()?;
        if root_folder_id.is_empty() {
            info!("[Google Drive] ROOT_FOLDER_IDが未設定のためスキップ");
            return None;
        }

        let credentials_file = std::env::var("GOOGLE_DRIVE_CREDENTIALS_FILE").ok()?;
        if credentials_file.is_empty() || !std::path::Path::new(&credentials_file).exists() {
            info!("[Google Drive] サービスアカウントキーが未設定のためスキップ");
            return None;
        }

        Some(DriveClient {
            credentials_file,
            root_folder_id,
        })
    }

    /// 指定した認証情報でクライアントを構築する
    pub fn new(credentials_file: impl Into<String>, root_folder_id: impl Into<String>) -> Self {
        DriveClient {
            credentials_file: credentials_file.into(),
            root_folder_id: root_folder_id.into(),
        }
    }

    /// ファイルを Google Drive にアップロードする
    ///
    /// # 引数
    /// - `folder_path`: フォルダ名の順序リスト。ルートフォルダ配下にこの階層で
    ///   フォルダを作成/検索し、最終フォルダにファイルをアップロードします。
    ///   例: `&["レジシステム", "2026-08"]`
    /// - `filename`: アップロードするファイル名
    /// - `file_bytes`: ファイルのバイナリデータ
    /// - `mime_type`: MIME タイプ (例: `"application/json"`, `"application/pdf"`)
    pub async fn upload_file(
        &self,
        folder_path: &[&str],
        filename: &str,
        file_bytes: &[u8],
        mime_type: &str,
    ) -> Result<UploadedFile> {
        let access_token = self.get_access_token().await?;

        // フォルダ階層を再帰的に作成/検索
        let mut current_parent_id = self.root_folder_id.clone();
        for folder_name in folder_path {
            current_parent_id =
                find_or_create_folder(&access_token, &current_parent_id, folder_name).await?;
        }

        // ファイルをアップロード
        let (file_id, web_view_link) =
            upload_file_multipart(&access_token, &current_parent_id, filename, file_bytes, mime_type)
                .await?;

        info!(
            "[Google Drive] アップロード成功: {} → {}",
            filename, file_id
        );

        Ok(UploadedFile {
            file_id,
            web_view_link,
        })
    }

    /// サービスアカウントのJWTからアクセストークンを取得
    async fn get_access_token(&self) -> Result<String> {
        let cred_json = std::fs::read_to_string(&self.credentials_file)?;
        let cred: serde_json::Value = serde_json::from_str(&cred_json)
            .map_err(|e| DriveError::CredentialsParse(format!("{}", e)))?;

        let client_email = cred["client_email"]
            .as_str()
            .ok_or_else(|| {
                DriveError::CredentialsParse("client_email not found in credentials".to_string())
            })?;
        let private_key = cred["private_key"]
            .as_str()
            .ok_or_else(|| {
                DriveError::CredentialsParse("private_key not found in credentials".to_string())
            })?;
        let token_uri = cred["token_uri"]
            .as_str()
            .unwrap_or("https://oauth2.googleapis.com/token");

        // JWT作成
        let now = Utc::now().timestamp();
        let claims = json!({
            "iss": client_email,
            "scope": "https://www.googleapis.com/auth/drive",
            "aud": token_uri,
            "iat": now,
            "exp": now + 3600,
        });

        let header = base64_url_encode(&json!({"alg": "RS256", "typ": "JWT"}).to_string());
        let payload = base64_url_encode(&claims.to_string());
        let signing_input = format!("{}.{}", header, payload);

        // RSA署名
        let key = openssl::pkey::PKey::private_key_from_pem(private_key.as_bytes())
            .map_err(|e| DriveError::AuthFailed(format!("秘密鍵解析エラー: {}", e)))?;
        let mut signer = openssl::sign::Signer::new(openssl::hash::MessageDigest::sha256(), &key)
            .map_err(|e| DriveError::AuthFailed(format!("署名初期化エラー: {}", e)))?;
        signer
            .update(signing_input.as_bytes())
            .map_err(|e| DriveError::AuthFailed(format!("署名更新エラー: {}", e)))?;
        let signature = signer
            .sign_to_vec()
            .map_err(|e| DriveError::AuthFailed(format!("署名エラー: {}", e)))?;

        let jwt = format!("{}.{}", signing_input, base64_url_encode_bytes(&signature));

        // アクセストークン取得
        let client = reqwest::Client::new();
        let resp = client
            .post(token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", &jwt),
            ])
            .send()
            .await?;

        let body: serde_json::Value = resp.json().await?;
        body["access_token"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| {
                DriveError::AuthFailed(format!("アクセストークン取得失敗: {}", body))
            })
    }
}

/// URL-safe Base64 エンコード（パディングなし）
fn base64_url_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes())
}

/// URL-safe Base64 エンコード（バイト列、パディングなし）
fn base64_url_encode_bytes(input: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input)
}

/// フォルダを Drive 内で検索し、なければ作成する
async fn find_or_create_folder(token: &str, parent_id: &str, name: &str) -> Result<String> {
    let client = reqwest::Client::new();

    // 検索: 同じ名前で同じ親フォルダ配下のフォルダを検索
    let query = format!(
        "name='{}' and '{}' in parents and mimeType='application/vnd.google-apps.folder' and trashed=false",
        name, parent_id
    );
    let resp: serde_json::Value = client
        .get("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(token)
        .query(&[("q", &query), ("fields", &"files(id)".to_string())])
        .send()
        .await?
        .json()
        .await?;

    if let Some(files) = resp["files"].as_array() {
        if let Some(first) = files.first() {
            if let Some(id) = first["id"].as_str() {
                return Ok(id.to_string());
            }
        }
    }

    // 作成: フォルダが存在しないので新規作成
    let metadata = json!({
        "name": name,
        "mimeType": "application/vnd.google-apps.folder",
        "parents": [parent_id],
    });
    let resp: serde_json::Value = client
        .post("https://www.googleapis.com/drive/v3/files")
        .bearer_auth(token)
        .json(&metadata)
        .send()
        .await?
        .json()
        .await?;

    resp["id"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| DriveError::ApiError(format!("フォルダ作成失敗: {}", resp)))
}

/// ファイルを Drive にアップロード（multipart）
async fn upload_file_multipart(
    token: &str,
    parent_id: &str,
    filename: &str,
    file_bytes: &[u8],
    mime_type: &str,
) -> Result<(String, String)> {
    let client = reqwest::Client::new();

    let metadata = json!({
        "name": filename,
        "parents": [parent_id],
    });

    // Multipart upload
    let form = reqwest::multipart::Form::new()
        .text("metadata", metadata.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(file_bytes.to_vec())
                .file_name(filename.to_string())
                .mime_str(mime_type)?,
        );

    let resp: serde_json::Value = client
        .post(
            "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart&fields=id,webViewLink",
        )
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await?
        .json()
        .await?;

    let file_id = resp["id"].as_str().unwrap_or("").to_string();
    let web_link = resp["webViewLink"].as_str().unwrap_or("").to_string();

    Ok((file_id, web_link))
}

/// Drive ファイルID からビューURL を生成する
pub fn drive_file_url(file_id: &str) -> String {
    if file_id.is_empty() {
        String::new()
    } else {
        format!("https://drive.google.com/file/d/{}/view", file_id)
    }
}
