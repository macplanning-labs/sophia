//! auth-core — SENN の認証ライブラリ
//!
//! JWT / TOTP / WebAuthn(パスキー) / パスワードの各認証方式と、
//! レート制限・試行回数ロックを提供する独立クレートです。
//!
//! ## 構成
//! - `domain::jwt` / `domain::totp` / `domain::webauthn` / `domain::password`
//!   が各認証方式の中核ロジックを持ちます。
//! - `infrastructure::rate_limit` は `tower_governor` ベースのIPレート制限を、
//!   `domain::attempt_lock` はアカウント単位の試行回数ロックを提供します。
//! - `domain::password_policy` / `domain::one_time_token` / `domain::mfa_policy` /
//!   `domain::audit` / `presentation` 配下は、利用側での組み込みを前提とした
//!   トレイト中心の構成になっています。
//!
//! ## 設計方針
//! auth-core はユーザーの永続化・ドメインモデルを知りません。DBアクセスが必要な箇所は
//! 必ずトレイト経由でアプリ側に委譲します(例: `jwt::TokenBlacklist`、
//! `one_time_token::OneTimeTokenStore`、`attempt_lock::AttemptStore`)。
//! sqlx のような特定のDBクレートには依存しません。

pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod presentation;

pub use error::{AuthError, Result};
