//! domain/webauthn.rs — WebAuthn（パスキー）
//!
//! WIPの `webauthn_service.rs` からほぼそのまま移植。
//!
//! 補足（Step 1調査メモ）: 方針ドキュメントのロードマップは「WIPの未実装である
//! Passkeyログイン検証を追加実装する」としているが、実際には移植元のWIPコードに
//! `start_authentication` / `identify_authentication` / `finish_authentication`
//! （discoverable credential方式によるログイン検証）が既に実装済みだった。
//! そのままauth-coreに移植したので、Step 1の時点で追加実装は不要。
//! ドキュメント側の記述更新を検討されたい。
//!
//! RP ID/Originの解決はSophiaが採用している「リクエストのHost/X-Forwarded-Proto
//! から動的解決する」方式を踏襲している（[`create_webauthn_from_headers`]）。

use axum::http::HeaderMap;
use url::Url;
use webauthn_rs::prelude::*;
use webauthn_rs::WebauthnBuilder;
// webauthn-rsのpreludeには含まれていないため直接importする
use webauthn_rs_proto::ResidentKeyRequirement;

use crate::error::AuthError;

/// WebAuthn設定を初期化してWebauthnインスタンスを返す。
///
/// - `rp_id` — Relying Party ID（ドメイン名、例: "localhost", "wip.example.com"）
/// - `rp_origin` — Relying Party Origin（例: "http://localhost:3000"）
/// - `rp_name` — 表示名（例: "WIP — プロジェクト管理ツール"）
pub fn create_webauthn(rp_id: &str, rp_origin: &str, rp_name: &str) -> Result<Webauthn, AuthError> {
    let origin = Url::parse(rp_origin)
        .map_err(|e| AuthError::WebAuthn(format!("無効なRP Origin '{rp_origin}': {e}")))?;

    let builder = WebauthnBuilder::new(rp_id, &origin)
        .map_err(|e| AuthError::WebAuthn(format!("WebAuthn設定エラー: {e}")))?;

    builder
        .rp_name(rp_name)
        .build()
        .map_err(|e| AuthError::WebAuthn(format!("WebAuthn構築エラー: {e}")))
}

/// リクエストヘッダーからRP ID / RP Originをその場で解決し、Webauthnインスタンスを
/// 都度構築する。固定の環境変数に頼ると環境ごとにズレやすいため、アクセスされた
/// Hostヘッダーからその場で組み立てることで環境ごとの設定ミスを無くす。
pub fn create_webauthn_from_headers(
    headers: &HeaderMap,
    rp_name: &str,
) -> Result<Webauthn, AuthError> {
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

    create_webauthn(rp_id, &rp_origin, rp_name)
}

// ── 登録（Registration）──

/// パスキー登録を開始する。
/// フロントエンドにCreationChallengeResponse（JSON）を返し、PasskeyRegistrationは
/// クライアントに返す（stateless/echo-backパターン）。
pub fn start_registration(
    webauthn: &Webauthn,
    user_id: Uuid,
    username: &str,
    display_name: &str,
    existing_credentials: Option<Vec<Passkey>>,
) -> Result<(CreationChallengeResponse, PasskeyRegistration), AuthError> {
    let exclude = existing_credentials.as_ref().map(|creds| {
        creds
            .iter()
            .map(|c| c.cred_id().clone())
            .collect::<Vec<_>>()
    });

    let (mut ccr, reg_state) = webauthn
        .start_passkey_registration(user_id, username, display_name, exclude)
        .map_err(|e| AuthError::WebAuthn(format!("パスキー登録開始に失敗: {e}")))?;

    // start_passkey_registrationはresidentKey: "discouraged"を送るため、
    // パスワードマネージャー等の認証器が非discoverable(非常駐)なクレデンシャルを
    // 作成してしまい、ログイン時のdiscoverable認証で一切見つからなくなる。
    // ログインにdiscoverable方式を使う以上、登録時に明示的にresidentKey: required
    // を要求する必要がある。
    if let Some(sel) = ccr.public_key.authenticator_selection.as_mut() {
        sel.resident_key = Some(ResidentKeyRequirement::Required);
        sel.require_resident_key = true;
    }

    Ok((ccr, reg_state))
}

/// パスキー登録を完了する。ブラウザから返却されたRegisterPublicKeyCredentialを
/// 検証し、成功すればPasskeyを返す（DBに保存する）。
pub fn finish_registration(
    webauthn: &Webauthn,
    reg_state: &PasskeyRegistration,
    credential: &RegisterPublicKeyCredential,
) -> Result<Passkey, AuthError> {
    webauthn
        .finish_passkey_registration(credential, reg_state)
        .map_err(|e| AuthError::WebAuthn(format!("パスキー登録完了に失敗: {e}")))
}

// ── 認証（Authentication / ログイン）──
//
// discoverable credential(ユーザー名不要のログイン)方式を採用。
// start_passkey_authentication/finish_passkey_authentication(全ユーザーのPasskeyを
// 事前にstart時にDBから読み込んで渡す方式)は、未認証の第三者に全ユーザーの
// credential_id一覧(allowCredentials)が開示されてしまうため使わない。
// 代わりに"conditional-ui" featureで提供されるstart_discoverable_authenticationを使う。
//
// フロー:
//   1. start_discoverable_authentication() — DBアクセスなし
//   2. クライアントから返ってきたPublicKeyCredentialに対しidentify_authentication()で
//      検証前に(user_unique_id, credential_id)を取り出す
//   3. そのuser_idのPasskeyだけをDBから取得し、DiscoverableKeyへ変換
//   4. finish_authentication()で検証

/// パスキーログインを開始する（discoverable、ユーザー名不要、DBアクセスなし）。
pub fn start_authentication(
    webauthn: &Webauthn,
) -> Result<(RequestChallengeResponse, DiscoverableAuthentication), AuthError> {
    webauthn
        .start_discoverable_authentication()
        .map_err(|e| AuthError::WebAuthn(format!("パスキー認証開始に失敗: {e}")))
}

/// クライアントから返ってきたPublicKeyCredentialから、検証前に
/// (ユーザーのUuid, credential_id)を取り出す。このUuidから対象ユーザーを特定し、
/// そのユーザーのPasskeyだけをDBから読み込んでfinish_authenticationに渡すこと
/// （全ユーザー分を読み込まない）。
pub fn identify_authentication(
    webauthn: &Webauthn,
    credential: &PublicKeyCredential,
) -> Result<(Uuid, Vec<u8>), AuthError> {
    webauthn
        .identify_discoverable_authentication(credential)
        .map(|(uuid, cred_id)| (uuid, cred_id.to_vec()))
        .map_err(|e| AuthError::WebAuthn(format!("パスキーの識別に失敗: {e}")))
}

/// パスキーログインを完了する。`creds`はidentify_authenticationで特定した
/// 対象ユーザーのPasskeyのみを渡すこと。
pub fn finish_authentication(
    webauthn: &Webauthn,
    auth_state: DiscoverableAuthentication,
    credential: &PublicKeyCredential,
    creds: &[Passkey],
) -> Result<AuthenticationResult, AuthError> {
    let discoverable_keys: Vec<DiscoverableKey> = creds.iter().map(DiscoverableKey::from).collect();
    webauthn
        .finish_discoverable_authentication(credential, auth_state, &discoverable_keys)
        .map_err(|e| AuthError::WebAuthn(format!("パスキー認証完了に失敗: {e}")))
}

// ── ユーティリティ（状態のJSONシリアライズ、echo-backパターン用）──

pub fn credential_id_from_passkey(passkey: &Passkey) -> Vec<u8> {
    passkey.cred_id().to_vec()
}

pub fn passkey_to_json_string(passkey: &Passkey) -> Result<String, AuthError> {
    serde_json::to_string(passkey)
        .map_err(|e| AuthError::WebAuthn(format!("Passkey JSONシリアライズに失敗: {e}")))
}

pub fn passkey_from_json_string(json_str: &str) -> Result<Passkey, AuthError> {
    serde_json::from_str(json_str)
        .map_err(|e| AuthError::WebAuthn(format!("Passkey JSONデシリアライズに失敗: {e}")))
}

pub fn registration_state_to_json_string(state: &PasskeyRegistration) -> Result<String, AuthError> {
    serde_json::to_string(state).map_err(|e| {
        AuthError::WebAuthn(format!("PasskeyRegistration JSONシリアライズに失敗: {e}"))
    })
}

pub fn registration_state_from_json_string(
    json_str: &str,
) -> Result<PasskeyRegistration, AuthError> {
    serde_json::from_str(json_str).map_err(|e| {
        AuthError::WebAuthn(format!("PasskeyRegistration JSONデシリアライズに失敗: {e}"))
    })
}

pub fn authentication_state_to_json_string(
    state: &DiscoverableAuthentication,
) -> Result<String, AuthError> {
    serde_json::to_string(state).map_err(|e| {
        AuthError::WebAuthn(format!(
            "DiscoverableAuthentication JSONシリアライズに失敗: {e}"
        ))
    })
}

pub fn authentication_state_from_json_string(
    json_str: &str,
) -> Result<DiscoverableAuthentication, AuthError> {
    serde_json::from_str(json_str).map_err(|e| {
        AuthError::WebAuthn(format!(
            "DiscoverableAuthentication JSONデシリアライズに失敗: {e}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_webauthn_localhost() {
        let result = create_webauthn("localhost", "http://localhost:3000", "Test App");
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_webauthn_invalid_origin() {
        let result = create_webauthn("example.com", "not-a-url", "Test App");
        assert!(result.is_err());
    }
}
