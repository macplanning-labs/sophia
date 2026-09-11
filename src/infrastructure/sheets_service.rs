/// infrastructure/sheets_service.rs — Google Sheets連携サービス（稼働報告自己申告用）
///
/// drive_service.rs と同じサービスアカウント（GOOGLE_DRIVE_CREDENTIALS_FILE）を使い、
/// スコープにスプレッドシートの読み書きを追加して利用する。
///
/// 用途: 案件に紐づかない社員向け「月次稼働報告（自己申告）」で、Googleスプレッドシートの
/// マスターテンプレート（GOOGLE_SHEETS_TIMESHEET_TEMPLATE_ID）を社員ごとに複製・共有し、
/// 記入後の値を読み取ってSophiaに取り込む。

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::info;

use super::drive_service::get_scoped_access_token;

const SCOPES: &str = "https://www.googleapis.com/auth/drive https://www.googleapis.com/auth/spreadsheets";

#[derive(Debug, Deserialize)]
struct DriveFile {
    id: String,
}

/// RFC3986の unreserved 文字以外をすべて%エンコードする（Sheets APIのrangeをURLパスに埋め込むため）。
/// 依存クレートを増やさないための最小実装。
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/// マスターテンプレート（`GOOGLE_SHEETS_TIMESHEET_TEMPLATE_ID`）を複製し、指定メールアドレスに
/// 編集権限を付与する。戻り値は (複製後のファイルID, 編集用URL)。
/// テンプレートID・サービスアカウントが未設定の場合はNoneを返す（呼び出し元は機能をスキップする）。
pub async fn copy_template_and_share(employee_email: &str, sheet_name: &str) -> Result<Option<(String, String)>> {
    let template_id = std::env::var("GOOGLE_SHEETS_TIMESHEET_TEMPLATE_ID").unwrap_or_default();
    if template_id.is_empty() {
        info!("[Google Sheets] GOOGLE_SHEETS_TIMESHEET_TEMPLATE_IDが未設定のためスキップ");
        return Ok(None);
    }

    let Some(token) = get_scoped_access_token(SCOPES).await? else {
        return Ok(None);
    };

    let client = reqwest::Client::new();

    // テンプレートを複製
    let copy_resp = client
        .post(format!("https://www.googleapis.com/drive/v3/files/{}/copy", template_id))
        .bearer_auth(&token)
        .query(&[("supportsAllDrives", "true"), ("fields", "id")])
        .json(&serde_json::json!({ "name": sheet_name }))
        .send()
        .await
        .context("スプレッドシートの複製リクエストに失敗")?;

    if !copy_resp.status().is_success() {
        let body = copy_resp.text().await.unwrap_or_default();
        anyhow::bail!("スプレッドシートの複製に失敗しました: {}", body);
    }
    let copied: DriveFile = copy_resp.json().await.context("スプレッドシートの複製レスポンスの解析に失敗")?;

    // 対象社員に編集権限を付与（通知メールは送らない。招待ではなく共有のみ）
    let perm_resp = client
        .post(format!("https://www.googleapis.com/drive/v3/files/{}/permissions", copied.id))
        .bearer_auth(&token)
        .query(&[("supportsAllDrives", "true"), ("sendNotificationEmail", "false")])
        .json(&serde_json::json!({
            "type": "user",
            "role": "writer",
            "emailAddress": employee_email,
        }))
        .send()
        .await
        .context("編集権限の付与リクエストに失敗")?;

    if !perm_resp.status().is_success() {
        let body = perm_resp.text().await.unwrap_or_default();
        anyhow::bail!("編集権限の付与に失敗しました: {}", body);
    }

    let url = format!("https://docs.google.com/spreadsheets/d/{}/edit", copied.id);
    info!("[Google Sheets] テンプレート複製・共有成功: {} → {} ({})", sheet_name, employee_email, copied.id);
    Ok(Some((copied.id, url)))
}

/// 指定範囲（A1記法。例: "稼働報告!A9:I39"）のセル値を読み取る。
/// サービスアカウントが未設定の場合はNoneを返す（呼び出し元は機能をスキップする）。
pub async fn read_values(file_id: &str, range: &str) -> Result<Option<Vec<Vec<serde_json::Value>>>> {
    let Some(token) = get_scoped_access_token(SCOPES).await? else {
        return Ok(None);
    };

    #[derive(Debug, Deserialize)]
    struct ValueRange {
        #[serde(default)]
        values: Vec<Vec<serde_json::Value>>,
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
            file_id,
            percent_encode(range),
        ))
        .bearer_auth(&token)
        .send()
        .await
        .context("スプレッドシートの値取得リクエストに失敗")?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("スプレッドシートの値取得に失敗しました: {}", body);
    }

    let value_range: ValueRange = resp.json().await.context("スプレッドシートの値レスポンスの解析に失敗")?;
    Ok(Some(value_range.values))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_leaves_unreserved_chars_untouched() {
        assert_eq!(percent_encode("abcXYZ019-_.~"), "abcXYZ019-_.~");
    }

    #[test]
    fn percent_encode_encodes_range_special_chars_and_japanese() {
        assert_eq!(percent_encode("稼働報告!A9:I39"), "%E7%A8%BC%E5%83%8D%E5%A0%B1%E5%91%8A%21A9%3AI39");
    }
}
