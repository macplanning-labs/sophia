//! auth-core — WIP / Sophia 共通認証ライブラリ
//!
//! 認証・共通基盤統合方針（プロジェクトドキュメント「認証ロジックのライブラリ化
//! （クレート化）およびWIP仕様への一元統一」）の Step 1 として、WIPリポジトリの
//! 既存実装（jwt_service / totp_service / webauthn_service /
//! middleware）を土台に切り出した独立クレートです。
//!
//! ## 現状（Step 1 時点）
//! - `domain::jwt` / `domain::totp` / `domain::webauthn` / `domain::password` は
//!   WIPの既存ロジックをほぼそのまま移植し、テストも合わせて移植しています。
//! - `infrastructure::rate_limit` は WIP の `tower_governor` ラッパーに加えて、
//!   Sophiaの `login_guard.rs` 相当である `attempt_lock` ミドルウェアを新規実装しています
//!   （方針ドキュメント 1.2 / 1.2.1 節）。
//! - `domain::password_policy` / `domain::one_time_token` / `domain::mfa_policy` /
//!   `domain::audit` / `presentation` 配下は、方針ドキュメント 2章のクレート構成・
//!   5〜7章の設計方針に基づく新規スケルトンです。トレイト境界は固めていますが、
//!   各アプリでの利用（Step 2: WIP適用 / Step 3: Sophia適用）を通じて実装が
//!   こなれていく想定です。
//! - auth-core はユーザーの永続化・ドメインモデルを知りません（7章の方針）。
//!   DBアクセスが必要な箇所は必ずトレイト経由でアプリ側に委譲します
//!   （例: `jwt::TokenBlacklist`、`one_time_token::OneTimeTokenStore`、
//!   `attempt_lock::AttemptStore`）。sqlxのような特定DBクレートには依存しません。

pub mod domain;
pub mod error;
pub mod infrastructure;
pub mod presentation;

pub use error::{AuthError, Result};
