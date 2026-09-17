# Sophia — SES受発注管理システム（OSS）

SES（システムエンジニアリングサービス）事業向けの受発注・稼働報告・給与計算を扱う Web アプリケーションの **公開ドンガラ** です。

> **公開方針:** アーキテクチャとコード構造のみ。実データ・ダミーシード・本番認証情報は同梱しません。

## セットアップ（推奨: Docker 一発）

ホストに Rust / Node / PostgreSQL の個別インストールは不要です。

```bash
cp .env.example .env
# .env の POSTGRES_PASSWORD と SECRET_KEY をローカル用の値に書き換える（初回起動前に）
docker compose up --build
```

ブラウザで http://localhost:8111 を開きます。初回起動時にマイグレーションが自動適用されます。ユーザーは管理画面から作成してください（初期シードユーザーは同梱しません）。

**つまずきやすい点:** PostgreSQL のデータは Docker ボリューム `sophia_pgdata` に残ります。`.env` の `POSTGRES_PASSWORD` を一度起動したあとに変えると認証エラーになります。パスワードを変える／まっさらにやり直すときは次を実行してください。

```bash
docker compose down -v   # ローカル DB ボリュームも削除
docker compose up --build
```

## デスクトップアプリ（準備中）

Tauri 製デスクトップ（macOS / Windows）のソースは `frontend/src-tauri/` にあります。ビルド手順は `.github/workflows/desktop-build.yml` を参照してください。

**現時点では Releases に一般向け .dmg / .exe を置いていません。** 一般利用の正本は上記 **Docker 一発起動** です。Desktop 配布物が揃い次第、Release assets として公開します。

## アーキテクチャ（要約）

```
DDD + axum（Rust）モノリス / frontend は React（Next.js 静的エクスポート）

src/
  domain/          # モデル・汎用サービス枠
  infrastructure/  # DB・外部アダプタ境界
  presentation/    # handlers / middleware
frontend/          # SPA
migrations/        # DDL の形（ビジネスデータ無し）
crates/
  auth-core/       # 認証共通（JWT / MFA 等）
  drive-core/      # Google Drive 連携共通（認証情報は env）
```

商流・料金・マージンの社内固有ロジックは公開対象外です（汎用の上下限／固定精算枠のみ）。

## ライセンス

MIT License（`LICENSE`）。依存の大半は MIT / Apache-2.0 系です。

## セキュリティ

- 秘密は環境変数のみ（`.env.example` はキー名とプレースホルダ）
- `docker-compose*.yml` の秘密は `${VAR}` のみ
- 公開前に gitleaks / `cargo audit` / `npm audit` を実施

## 開発者向け（任意）

ホストで動かす場合は Rust・Node・PostgreSQL が必要です。`scripts/dev.sh` を参照してください。OSS 利用者は上記の `docker compose up --build` を推奨します。

## やらないこと（本リポジトリ）

- 実 DB ダンプ・取引先マスタ・契約実データの同梱
- 常設デモ／LP（別サイクル）
- 社内本番 runbook の機密節
