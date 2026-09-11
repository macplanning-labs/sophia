/// domain/services/webauthn_service.rs — WebAuthn（パスキー）サービス
///
/// webauthn-rs を使用したパスキーの登録・認証。
/// Phase 1: 認証基盤 + MFA の一部。
///
/// ## チャレンジ管理の方針
/// Passkey 中間状態（チャレンジ）は **DB テーブル + Cookie に challenge_id のみ** で管理する。
/// MemoryStore やプロセス内ストアは採用しない（マルチプロセス・インスタンス再起動・複数インスタンス環境で状態消失）。
/// 詳細は `docs/spec/アプリ方式設計書.md` の認証節を参照。
///
/// ## 設計
/// - Webauthn インスタンスは AppState に保持して全ハンドラで共有
/// - registration/authentication の中間状態（PasskeyRegistration/PasskeyAuthentication）は
///   DB テーブル（s_passkey_login_challenge など）に JSON で保存し、challenge_id を Cookie で追跡
/// - 完成したクレデンシャルは s_webauthn_credential テーブルに JSONB で保存

use axum::http::HeaderMap;
use url::Url;
use webauthn_rs::prelude::*;
use webauthn_rs::WebauthnBuilder;

/// WebAuthn 設定を初期化して Webauthn インスタンスを返す
///
/// # Arguments
/// - `rp_id` — Relying Party ID（ドメイン名、例: "localhost", "sophia.example.com"）
/// - `rp_origin` — Relying Party Origin（例: "http://localhost:8111"）
pub fn create_webauthn(rp_id: &str, rp_origin: &str) -> anyhow::Result<Webauthn> {
    let origin = Url::parse(rp_origin)
        .map_err(|e| anyhow::anyhow!("無効な RP Origin '{}': {}", rp_origin, e))?;

    let builder = WebauthnBuilder::new(rp_id, &origin)
        .map_err(|e| anyhow::anyhow!("WebAuthn設定エラー: {}", e))?;

    let webauthn = builder
        .rp_name("Sophia — SES受発注管理")
        .build()
        .map_err(|e| anyhow::anyhow!("WebAuthn構築エラー: {}", e))?;

    Ok(webauthn)
}

/// リクエストヘッダーから RP ID / RP Origin をその場で解決し、Webauthn インスタンスを
/// 都度構築する（旧EDI_MP(Django)の `request.get_host()` 方式を踏襲）。
///
/// 固定の WEBAUTHN_RP_ID/RP_ORIGIN 環境変数に頼ると、ローカル開発・ステージング・本番で
/// ポートやホスト名が異なるたびに設定を合わせ込む必要があり、ズレたまま気づかれずに
/// 「登録は成功するがorigin不一致で失敗する」といった不具合を生みやすい。
/// アクセスされた Host ヘッダー（nginx が `$http_host` で転送、ポート込み）から
/// その場で組み立てることで、環境ごとの設定ミスを構造的に無くす。
pub fn create_webauthn_from_headers(headers: &HeaderMap) -> anyhow::Result<Webauthn> {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("localhost");
    let scheme = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("http");

    let rp_id = host.split(':').next().unwrap_or(host);
    let rp_origin = format!("{scheme}://{host}");

    create_webauthn(rp_id, &rp_origin)
}

// ── 登録（Registration）──

/// パスキー登録を開始する
///
/// フロントエンドに CreationChallengeResponse（JSON）を返し、
/// PasskeyRegistration はセッションに保存する。
pub fn start_registration(
    webauthn: &Webauthn,
    user_id: Uuid,
    username: &str,
    display_name: &str,
    existing_credentials: Option<Vec<Passkey>>,
) -> anyhow::Result<(CreationChallengeResponse, PasskeyRegistration)> {
    let exclude = existing_credentials
        .as_ref()
        .map(|creds| creds.iter().map(|c| c.cred_id().clone()).collect::<Vec<_>>());

    webauthn.start_passkey_registration(
        user_id,
        username,
        display_name,
        exclude,
    )
    .map_err(|e| anyhow::anyhow!("パスキー登録開始に失敗: {}", e))
}

/// パスキー登録を完了する
///
/// ブラウザから返却された RegisterPublicKeyCredential を検証し、
/// 成功すれば Passkey を返す（DB に保存する）。
pub fn finish_registration(
    webauthn: &Webauthn,
    reg_state: &PasskeyRegistration,
    credential: &RegisterPublicKeyCredential,
) -> anyhow::Result<Passkey> {
    webauthn.finish_passkey_registration(credential, reg_state)
        .map_err(|e| anyhow::anyhow!("パスキー登録完了に失敗: {}", e))
}

// ── 認証（Authentication）──

/// パスキー認証を開始する
///
/// フロントエンドに RequestChallengeResponse（JSON）を返し、
/// PasskeyAuthentication はセッションに保存する。
pub fn start_authentication(
    webauthn: &Webauthn,
    credentials: &[Passkey],
) -> anyhow::Result<(RequestChallengeResponse, PasskeyAuthentication)> {
    webauthn.start_passkey_authentication(credentials)
        .map_err(|e| anyhow::anyhow!("パスキー認証開始に失敗: {}", e))
}

/// パスキー認証を完了する
///
/// ブラウザから返却された PublicKeyCredential を検証し、
/// 成功すれば AuthenticationResult を返す。
/// 戻り値の cred_id と auth_data でDB側の sign_count を更新する。
pub fn finish_authentication(
    webauthn: &Webauthn,
    auth_state: &PasskeyAuthentication,
    credential: &PublicKeyCredential,
) -> anyhow::Result<AuthenticationResult> {
    webauthn.finish_passkey_authentication(credential, auth_state)
        .map_err(|e| anyhow::anyhow!("パスキー認証完了に失敗: {}", e))
}

// ── ユーティリティ ──

/// Passkey の credential_id を base64url エンコードする（DB保存用）
pub fn credential_id_to_string(passkey: &Passkey) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(passkey.cred_id())
}

/// Passkey 型を JSON 文字列にシリアライズする（DB保存用）
pub fn passkey_to_json(passkey: &Passkey) -> anyhow::Result<serde_json::Value> {
    serde_json::to_value(passkey)
        .map_err(|e| anyhow::anyhow!("Passkey JSON シリアライズに失敗: {}", e))
}

/// JSON から Passkey 型を復元する（DB読み込み用）
pub fn passkey_from_json(json: &serde_json::Value) -> anyhow::Result<Passkey> {
    serde_json::from_value(json.clone())
        .map_err(|e| anyhow::anyhow!("Passkey JSON デシリアライズに失敗: {}", e))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_webauthn_localhost() {
        let result = create_webauthn("localhost", "http://localhost:8111");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_webauthn_invalid_origin() {
        let result = create_webauthn("example.com", "not-a-url");
        assert!(result.is_err());
    }
}
