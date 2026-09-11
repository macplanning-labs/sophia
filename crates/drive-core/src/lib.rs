//! drive-core — Sophia・将来の他システム共通 Google Drive連携ライブラリ
//!
//! このクレートは Sophia の `src/domain/services/drive_service.rs` （自体は Django版
//! `core/domain/services/drive_service.py` の移植）を土台に切り出した、
//! サービスアカウント認証による Google Drive 連携を提供する共通ライブラリです。
//!
//! ## Sophia の実装からの一般化
//!
//! Sophia の `drive_service.rs` は以下の2点を固定化していました。
//! このクレートではこれらを汎用パラメータ化しました。
//!
//! 1. **フォルダ階層**: Sophia では `パートナー管理`/`クライアント管理` →
//!    会社名 → ドキュメント種別（`契約書`/`注文書` など）の2層固定階層でした。
//!    このクレートでは、フォルダ名のスライス `folder_path: &[&str]` を受け取り、
//!    任意の階層構造に対応できるようにしました。
//!    例えば別のアプリでは
//!    `&["レジシステム", "2026-08"]` のように階層を指定して使用します。
//!
//! 2. **MIME タイプ**: Sophia では `upload_file()` で MIME タイプを
//!    `"application/pdf"` に固定化していました。
//!    このクレートでは `mime_type: &str` パラメータにして、
//!    JSON / CSV など異なるファイル形式にも対応できるようにしました。
//!
//! ## 認証・アップロードロジック
//!
//! JWT 生成（RS256署名）→ Google OAuth2 トークン交換 → Drive API v3 への
//! REST 呼び出しという認証・通信フローは、Sophia での実装実績に基づいて
//! 変更していません。多くの企業の本番環境で動いているコードだからです。
//!
//! 本クレートのユーザーは、`DriveClient::upload_file()` を呼ぶ形で使用します。
//! 環境変数 `GOOGLE_DRIVE_ROOT_FOLDER_ID` と `GOOGLE_DRIVE_CREDENTIALS_FILE`
//! が未設定の場合、スキップして空文字列を返します（Sophia の既存動作を踏襲）。
//!
//! ## 未実装・スコープ外
//!
//! - **Sophia の既存呼び出し箇所の移行**: Sophia リポジトリのコードは
//!   `upload_order_pdf` / `upload_payment_notice_pdf` などの app-specific
//!   ラッパー関数をそのまま保持しており、このクレートへの移行は行っていません。
//!   Sophia の呼び出し側の変更は別途対応予定です。
//! - **フォルダラベル・命名ロジック**: Sophia の `DOC_TYPE_LABELS`
//!   （`contract` → `契約書` など）や固定階層 は このクレートに含みません。
//!   各アプリが必要に応じて薄いラッパー関数を書いてください。

pub mod error;
pub mod infrastructure;

pub use error::{DriveError, Result};
pub use infrastructure::drive_client::{DriveClient, UploadedFile};
