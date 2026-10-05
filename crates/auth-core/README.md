# auth-core

WIP・Sophia、および将来の他Rustサービス共通の認証ライブラリ。

詳細な方針・設計背景は Claude Project「認証・共通基盤統合」内のドキュメント
「認証ロジックのライブラリ化（クレート化）およびWIP仕様への一元統一」を参照。

## Step 1（このクレートの新設）で行ったこと

- WIPの既存実装（`jwt_service.rs` / `totp_service.rs` / `webauthn_service.rs` /
  `middleware/rate_limiter.rs`、`auth_service.rs`のパスワード/TOTP部分）を
  `domain::jwt` / `domain::totp` / `domain::webauthn` / `domain::password` /
  `infrastructure::rate_limit` に移植（既存の単体テストも合わせて移植）。
- 方針ドキュメント1.2/1.2.1節の設計に基づき、アカウント単位の試行回数ロック
  ミドルウェア（`infrastructure::rate_limit::attempt_lock_middleware`）を新規実装。
- 方針ドキュメント5〜7章の設計に基づき、`domain::password_policy` /
  `domain::one_time_token` / `domain::mfa_policy` / `domain::audit` /
  `presentation::extractors` / `presentation::middleware` をスケルトンとして新規追加。
- `rust/Cargo.toml` を「ルートパッケージ（`wip`）を維持したままのワークスペース」
  に変更し、このクレートを `[workspace] members = ["auth-core"]` として追加。
  既存の `wip` パッケージのディレクトリ構成・Dockerfile・CI（`ci.yml`）は
  変更していない（`cargo build` / `cargo test` はワークスペース全体を対象にする
  ため、`ci.yml` の既存ジョブがそのままこのクレートも検証する）。

## Step 1時点で見つかった、方針ドキュメントとの相違点と決定事項

Step1で実コードを確認した結果判明した相違点を踏まえ、2026-08-14に以下を決定した
（詳細は方針ドキュメント1.3〜1.5節、プロジェクト内「Step1実装ログ」を参照）。

1. **WIPは現在デュアル認証構成 → Web UI側もJWTへ統一する（決定）**: 方針ドキュメント
   1章の比較表はWIP=「JWT（アクセス30分/リフレッシュ7日）+ ブラックリスト」のみと
   していたが、実際のWIPはWeb UI向けにセッションCookie認証（`tower-sessions`、
   `middleware/auth.rs`の`require_auth`）も別途持っており、JWT Bearer認証
   （`middleware/jwt_auth.rs`）はAPIルート専用だった。Web UI側もhttpOnly Secure
   クッキー配布のJWTへ統一する方針が決定した（方針1.3節）。`must_change_password`/
   `mfa_pending`のセッション依存状態の置き換え方はStep2で詳細設計する。
2. **JWTクレーム形状がDjango依存 → Djangoは段階的に廃止する（決定）**: WIPのJWTは
   Django（`rest_framework_simplejwt`）と同一の`SECRET_KEY`で相互検証できるよう、
   クレーム形状（`token_type`/`jti`/`user_id`を文字列化 等）をDjango仕様に固定
   している。方針ドキュメント7章の汎用`Claims{sub,roles,exp,iss,extra}`とは非互換
   のため、`domain::jwt`では両方を実装し、Django互換層を`django_compat`モジュール
   として分離した。Djangoを段階的に廃止する方針が決定したため、`django_compat`は
   移行期間限定のブリッジと正式に位置付けている（方針1.4節）。
3. **Passkeyログイン検証は実装済みだった**: ロードマップは「WIPの未実装である
   Passkeyログイン検証を追加実装する」としていたが、実際には
   `webauthn_service.rs`に discoverable credential 方式のログイン検証
   （`start_authentication`/`identify_authentication`/`finish_authentication`）が
   既に実装済みだった。そのまま`domain::webauthn`に移植したので、Step 1時点で
   追加実装は不要だった。ロードマップ側の記述は方針ドキュメント4章で修正済み。
4. **CIはワークスペース全体を対象に実行される**: Cargoワークスペースの
   `cargo build` / `cargo test`はワークスペース全体に対して実行されるため、
   `auth-core`が追加された時点で自動的にビルド・テスト対象へ入る。
5. **TOTPの秘密鍵保存方式が2系統ある → AES-GCM暗号化保存に統一する（決定）**:
   `totp_service.rs`のAES-GCM暗号化保存（新規Rust実装向け）と、`auth_service.rs`
   のBase32平文検証（DjangoのTotpDeviceテーブルと共有、pyotp互換）が並存していた。
   AES-GCM暗号化保存を正式な標準とし、Base32平文検証（`verify_code_base32`）は
   Django稼働中の後方互換のためだけに残す方針が決定した（方針1.5節）。既存の
   平文シークレットの移行バッチはStep2で実装する。
6. **`domain::one_time_token`はSophia実コード未参照のスケルトン**: この
   セッションではSophiaリポジトリのパートナー認証トークン実装を直接参照できなかったため、方針ドキュメント5章の
   記述からトレイト設計のみ起こしてある。Step 3で実装を突き合わせること。

## 未着手（Step 2以降）

- WIPの21ファイル（`presentation/handlers/*_api.rs`等）が参照している
  `jwt_service` / `totp_service` / `webauthn_service` / `middleware::jwt_auth` /
  `middleware::rate_limiter`の呼び出し箇所を`auth-core`経由に差し替える作業
  （ロードマップ上も明示的にStep 2の範囲）。
- `jwt_blacklist_repo.rs`（sqlx実装）に`domain::jwt::TokenBlacklist`トレイトを
  実装させる配線。
- Sophia用の`LegacyHashVerifier`（bcrypt）実装。
