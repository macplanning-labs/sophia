# CHANGELOG

本プロジェクトは [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) 形式と SemVer に従います。

## [0.2.0] — 2026-09-11

### Added

- デスクトップアプリ（Tauri, macOS / Windows）のβ版を追加。GitHub Actions で自動ビルド・配布
- ダウンロード用ランディングページを追加

### Changed

- ログイン画面にパスワード表示切り替えを追加
- デスクトップアプリからの本番API利用のためCORS許可を追加

### Fixed

- MFA画面でセッション切れモーダルが認証コード入力を妨げる不具合を修正

### Security

- フロントエンド依存関係（Next.js 等）を更新し、既知の脆弱性を解消
- 非公開作業に紐づく内部限定ドキュメント（開発プロセス上のメモ・設計下書き）を履歴から除去

## [0.1.0] — 2026-09-06

### Added

- SES 受発注ドンガラの初回 OSS 公開（実データ非同梱）
- `docker compose up --build` による DB + アプリ一発起動
- MIT ライセンス、`crates/auth-core` / `crates/drive-core` の path 同梱

### Security

- 私有 git 依存を除去し、公開ビルド可能な構成へ
- 依存の既知 High 脆弱性を更新／除去（詳細はリリースノート）
