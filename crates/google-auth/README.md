# google-auth

Google Workspace（サービスアカウント + ドメイン全体委任）JWT認証の共通ライブラリ。

## 背景

Gmail API サービスアカウント認証ロジックを共通化したもの。
以下のようなサービスで同じロジックを使う場合に活用できます:

- `src/infrastructure/drive_service.rs`（Google Drive連携）
- `src/infrastructure/sheets_service.rs`（Google Sheets連携）
- `src/domain/services/email_service.rs`（Gmail API送信）

このクレートは DB・HTTPフレームワークに一切依存しない純粋な認証ロジックのみを提供します。
スコープとドメイン全体委任（`impersonate`）先を引数で指定できるため、Drive / Sheets / Gmail など任意の Google API に共用できます。

## 使用方法

```rust
use google_auth::get_access_token_from_file;

// 環境変数 GOOGLE_MAIL_CREDENTIALS_FILE で指定した認証情報ファイルから、
// from_email になりすまして gmail.send スコープのアクセストークンを取得する。
let token = get_access_token_from_file(
    "GOOGLE_MAIL_CREDENTIALS_FILE",
    Some("noreply@example.com"),
    "https://www.googleapis.com/auth/gmail.send",
).await?;
```

- `Ok(Some(token))`: 認証情報が設定・読み込め、トークン取得成功
- `Ok(None)`: 環境変数未設定または認証情報ファイル不存在（呼び出し元は機能をスキップする）
- `Err(e)`: 認証情報ファイルの読み込み・パース・トークン取得の失敗

サービスアカウントJSONキーを直接持っている場合は `get_access_token(&key, impersonate, scope)` を使うこともできます。
