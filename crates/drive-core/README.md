# drive-core

Sophia、および将来の他Rustサービス共通の Google Drive 連携ライブラリ。

## 概要

Sophia の `src/domain/services/drive_service.rs` （自体は Django版 `core/domain/services/drive_service.py` の移植）を土台に、サービスアカウント認証による Google Drive ファイルアップロード機能を提供する共通クレートです。

## Sophia の実装からの一般化

元々 Sophia では以下の2点を固定化していました。このクレートではこれらを汎用化しました：

1. **フォルダ階層**: Sophia は `パートナー管理`/`クライアント管理` → 会社名 → ドキュメント種別（`契約書` など）の固定2層階層を使用していました。このクレートでは `folder_path: &[&str]` パラメータで任意の階層を指定できるようにしました。

2. **MIME タイプ**: Sophia は アップロード時に MIME タイプを `"application/pdf"` に固定していました。このクレートでは `mime_type: &str` パラメータで JSON、CSV など異なるファイル形式に対応させました。

## 使用方法

```rust
use drive_core::DriveClient;

// 環境変数から初期化（GOOGLE_DRIVE_ROOT_FOLDER_ID, GOOGLE_DRIVE_CREDENTIALS_FILE）
if let Some(client) = DriveClient::from_env() {
    let result = client.upload_file(
        &["レジシステム", "2026-08"],           // フォルダ階層
        "transactions_20260828.json",            // ファイル名
        &json_bytes,                             // ファイルバイナリ
        "application/json",                      // MIME タイプ
    ).await;
    
    match result {
        Ok(uploaded) => println!("アップロード成功: {}", uploaded.file_id),
        Err(e) => eprintln!("エラー: {}", e),
    }
}
```

## 環境変数

- `GOOGLE_DRIVE_ROOT_FOLDER_ID`: Google Drive の ルートフォルダID（アップロード先の基準）
- `GOOGLE_DRIVE_CREDENTIALS_FILE`: Google Service Account キーファイルのパス

いずれか欠けている場合、`from_env()` は `None` を返します（スキップ扱い）。

## 注

- 認証・アップロードロジック（JWT RS256署名 → OAuth2 トークン交換 → Drive API v3 REST呼び出し）は Sophia での実装実績に基づくため、変更していません。
- 各アプリが app-specific なラッパー関数（例: `upload_transaction_json`）を書く場合は、このクレートの `DriveClient::upload_file()` をラップしてください。
