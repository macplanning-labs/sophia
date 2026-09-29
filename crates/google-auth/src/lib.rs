//! google-auth — Google Workspace サービスアカウント JWT 認証ライブラリ
//!
//! DB・HTTPフレームワークに依存しない純粋な認証ロジックのみを提供する。
//! スコープとドメイン全体委任（impersonate）先を引数で指定するため、
//! Drive / Sheets / Gmail など任意の Google API に共用できる。

pub mod error;
pub mod infrastructure;

pub use error::{GoogleAuthError, Result};
pub use infrastructure::token::{get_access_token, get_access_token_from_file, ServiceAccountKey};
