# Sophia — SES受発注管理システム（OSS）

SES（システムエンジニアリングサービス）事業向けの受発注・稼働報告・給与計算を扱う Web アプリケーションの **公開ドンガラ** です。

> **公開方針:** アーキテクチャとコード構造のみ。実データ・ダミーシード・本番認証情報は同梱しません。

## セットアップ（推奨: Docker 一発）

ホストに Rust / Node / PostgreSQL の個別インストールは不要です。

**必要なメモリ:** 初回のビルド(Rust のリリースビルド)は多くのメモリを使います。Docker に **4GB 以上（推奨 6GB 以上）** のメモリを割り当ててください。不足すると、ビルドが `cannot allocate memory` で失敗します（Docker Desktop の Settings → Resources、colima の場合は `colima start --memory 6`）。

```bash
cp .env.example .env
# .env の POSTGRES_PASSWORD と SECRET_KEY は空欄。各自で作った値を入れる（例: openssl rand -hex 32。初回起動前に）
docker compose up --build
```

ブラウザで http://localhost:8111 を開きます。初回起動時にマイグレーションが自動適用されます。初期ユーザーは同梱していないため、最初の管理者の作り方は次の節を参照してください。

### 最初の管理者を作る（初回のみ）

初回の起動の前に、`.env` の次の行のコメントを外して値を設定します（`.env.example` にあります）。

```bash
INITIAL_ADMIN_EMAIL=admin@example.com
INITIAL_ADMIN_PASSWORD=<12文字以上。英大文字・英小文字・数字を含める>
INITIAL_ADMIN_USERNAME=管理者   # 省略可
```

`docker compose up --build` で起動すると、**ユーザーが 1 人もいない場合に限り**、この管理者が作成されます（起動ログに「初期管理者を作成しました」と出ます）。

1. http://localhost:8111 を開き、設定したメールアドレス（大文字・小文字も入力したとおりに）とパスワードでログインします。
2. 画面の案内に従って、**多要素認証（MFA。TOTP またはパスキー）を登録します。登録するまで、ユーザーの追加などの管理者向け操作はできません。**
3. 管理画面から、必要なユーザーを追加します。
4. 管理者を作成できたら、`.env` から `INITIAL_ADMIN_*` の行を削除し、コンテナを作り直します（パスワードがコンテナの環境変数に残らないようにするため）。

   ```bash
   docker compose up -d --force-recreate
   ```

すでにユーザーがいる環境では、`INITIAL_ADMIN_*` を設定しても何も作られません（起動ログに警告が出ます）。パスワードや形式が不正な場合も作成されず、起動ログにエラーが出ます（サーバー自体は起動します）。

環境変数を使いたくない場合は、起動後に次のコマンドでも作成できます（パスワードは標準入力から渡し、コマンドの引数には書きません）。ユーザーが既にいれば何もしません。

```bash
echo '<パスワード>' | docker compose exec -T web sophia create-admin --email admin@example.com --password-stdin
```

**つまずきやすい点:** PostgreSQL のデータは Docker ボリューム `sophia_pgdata` に残ります。`.env` の `POSTGRES_PASSWORD` を一度起動したあとに変えると認証エラーになります。パスワードを変える／まっさらにやり直すときは次を実行してください。

```bash
docker compose down -v   # ローカル DB ボリュームも削除
docker compose up --build
```

## 任意機能（既定は無効）

連携機能は既定では無効です。`.env` の該当行のコメントを外して設定すると有効になります（`docker compose up` は `.env` を読み込みます）。SMTP による送信は本番以外の環境では誤送信防止のためテスト宛先へ転送されます。

| 機能 | 既定 | 有効にするには | 設定の場所 |
|---|---|---|---|
| メール送信（SMTP） | 無効（本番以外ではテスト宛先へ転送） | `ENV_NAME=production`、送信先ドメインは `EMAIL_INTERNAL_DOMAINS` | 画面「自社情報」（SMTP）+ 環境変数 |
| メール自動取込（IMAP） | 無効 | `MAIL_PIPELINE_ENABLED=true`、`IMAP_HOST`、画面「自社情報」の SMTP ユーザー/パスワード | 画面 + 環境変数 |
| Google Drive 保存 | 無効（鍵が無ければスキップ） | `GOOGLE_DRIVE_CREDENTIALS_FILE`（コンテナ内パス。volumes で鍵ファイルを渡す）、`GOOGLE_DRIVE_ROOT_FOLDER_ID` 他 | 環境変数 |
| EDI-OASIS 連携 | 無効（未設定なら「未設定」エラー） | `EDI_OASIS_BASE_URL` `EDI_OASIS_USER` `EDI_OASIS_PASSWORD` `EDI_OASIS_COMPANY_NAME` | 環境変数 |
| Peppol | 無効（同上） | `PEPPOL_API_BASE_URL` `PEPPOL_API_KEY` `PEPPOL_OWN_PARTICIPANT_ID` `PEPPOL_WEBHOOK_SECRET` | 環境変数 |
| Google Chat 通知 | 無効 | `GOOGLE_CHAT_WEBHOOK_URL` | 環境変数 |
| Ollama（ローカル AI） | 無効 | `OLLAMA_HOST` `OLLAMA_MODEL` | 環境変数 |

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
