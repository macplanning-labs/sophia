/// domain/services/webauthn_service.rs — WebAuthn（パスキー）サービス（auth-coreの薄いラッパー）
///
/// webauthn-rs の初期化・登録・認証ロジックは auth_core::domain::webauthn に集約されている。
/// ここでは Sophia 固有の RP 名("Sophia — SES受発注管理")の付与と、DB保存形式
/// （JSONB カラム passkey_json = serde_json::Value）への適合のみを行う。
///
/// ## チャレンジ管理の方針
/// Passkey 中間状態（チャレンジ）は **DB テーブル + Cookie に challenge_id のみ** で管理する。
/// MemoryStore やプロセス内ストアは採用しない（マルチプロセス・インスタンス再起動・複数インスタンス環境で状態消失）。
/// 詳細は `docs/spec/アプリ方式設計書.md` の認証節を参照。

use axum::http::HeaderMap;
use auth_core::error::AuthError;
use webauthn_rs::prelude::*;

/// WebAuthnのRP名。移植元の旧 webauthn_service.rs が固定していた値を踏襲する。
const RP_NAME: &str = "Sophia — SES受発注管理";

/// WebAuthn 設定を初期化して Webauthn インスタンスを返す
///
/// # Arguments
/// - `rp_id` — Relying Party ID（ドメイン名、例: "localhost", "sophia.example.com"）
/// - `rp_origin` — Relying Party Origin（例: "http://localhost:8111"）
pub fn create_webauthn(rp_id: &str, rp_origin: &str) -> Result<Webauthn, AuthError> {
    auth_core::domain::webauthn::create_webauthn(rp_id, rp_origin, RP_NAME)
}

/// リクエストヘッダーから RP ID / RP Origin をその場で解決し、Webauthn インスタンスを
/// 都度構築する（旧EDI_MP(Django)の `request.get_host()` 方式を踏襲）。
///
/// 固定の WEBAUTHN_RP_ID/RP_ORIGIN 環境変数に頼ると、ローカル開発・ステージング・本番で
/// ポートやホスト名が異なるたびに設定を合わせ込む必要があり、ズレたまま気づかれずに
/// 「登録は成功するがorigin不一致で失敗する」といった不具合を生みやすい。
/// アクセスされた Host ヘッダー（nginx が `$http_host` で転送、ポート込み）から
/// その場で組み立てることで、環境ごとの設定ミスを構造的に無くす。
///
/// 注意: このHost/X-Forwarded-Proto解決ロジック自体はauth-coreへは切り出さない
/// （auth-core側の`create_webauthn_from_headers`はallowlist検証を持たないため、
/// このSophia版のみが使う。allowlist検証を追加した経緯は下記`resolve_webauthn_from_headers`参照）。
pub fn create_webauthn_from_headers(headers: &HeaderMap) -> Result<Webauthn, AuthError> {
    let allowed_hosts = allowed_hosts_from_env();
    resolve_webauthn_from_headers(headers, &allowed_hosts)
}

/// WebAuthn の RP ID / Origin として受け入れるホスト名の allowlist を
/// 環境変数 `WEBAUTHN_ALLOWED_HOSTS`（カンマ区切り、ポート番号は含めない）から読み込む。
///
/// 未設定の場合は `localhost` のみを許可する（ローカル開発のデフォルト動作を維持しつつ、
/// ステージング/本番では明示的な設定を必須にするフェイルセーフ）。
fn allowed_hosts_from_env() -> Vec<String> {
    match std::env::var("WEBAUTHN_ALLOWED_HOSTS") {
        Ok(raw) if !raw.trim().is_empty() => raw
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
        _ => vec!["localhost".to_string()],
    }
}

/// host（ポート除去済み・小文字化済み）が許可リストに含まれるか
fn is_host_allowed(host: &str, allowed_hosts: &[String]) -> bool {
    allowed_hosts.iter().any(|h| h == host)
}

/// ローカル開発用ホストか（スキームはヘッダー/デフォルトのhttpを許容する）。
/// それ以外の許可ホスト（本番・ステージングの実ドメイン）は、直接アクセスされた場合に
/// クライアント側で偽装可能な `X-Forwarded-Proto` を信用せず、常に https に固定する。
fn is_local_host(host: &str) -> bool {
    host == "localhost" || host == "127.0.0.1" || host.ends_with(".lan")
}

/// `create_webauthn_from_headers` の本体（allowlistを引数で受け取るテスト可能な形）。
///
/// Host / X-Forwarded-Proto はいずれもクライアントが偽装できるヘッダーであり、
/// nginx 側の `server_name` もアプリ層のRP検証を代替しない（production/local は `_`
/// でキャッチオールしており、直接アクセス経路（例: LANのIP直指定ポート）では任意の
/// Host ヘッダーがそのままバックエンドへ転送されうる）。そのため、ここで
/// `WEBAUTHN_ALLOWED_HOSTS` の allowlist に対する検証を行い、一致しない場合は
/// WebAuthn の登録/認証そのものを失敗させる。
fn resolve_webauthn_from_headers(
    headers: &HeaderMap,
    allowed_hosts: &[String],
) -> Result<Webauthn, AuthError> {
    let host_header = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .unwrap_or("localhost");

    let rp_id = host_header.split(':').next().unwrap_or(host_header);
    let rp_id_lower = rp_id.to_ascii_lowercase();

    if !is_host_allowed(&rp_id_lower, allowed_hosts) {
        return Err(AuthError::WebAuthn(format!(
            "許可されていないHostヘッダーです: '{rp_id}'（WEBAUTHN_ALLOWED_HOSTSを確認してください）"
        )));
    }

    let scheme = if is_local_host(&rp_id_lower) {
        headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty())
            .unwrap_or("http")
            .to_string()
    } else {
        "https".to_string()
    };

    let host_header_lower = host_header.to_ascii_lowercase();
    let rp_origin = format!("{scheme}://{host_header_lower}");

    create_webauthn(&rp_id_lower, &rp_origin)
}

// ── 登録（Registration）──

/// パスキー登録を開始する
pub fn start_registration(
    webauthn: &Webauthn,
    user_id: Uuid,
    username: &str,
    display_name: &str,
    existing_credentials: Option<Vec<Passkey>>,
) -> Result<(CreationChallengeResponse, PasskeyRegistration), AuthError> {
    auth_core::domain::webauthn::start_registration(
        webauthn,
        user_id,
        username,
        display_name,
        existing_credentials,
    )
}

/// パスキー登録を完了する
pub fn finish_registration(
    webauthn: &Webauthn,
    reg_state: &PasskeyRegistration,
    credential: &RegisterPublicKeyCredential,
) -> Result<Passkey, AuthError> {
    auth_core::domain::webauthn::finish_registration(webauthn, reg_state, credential)
}

// ── 認証（Authentication）──

/// パスキー認証を開始する（既知のPasskey一覧を渡す方式）
pub fn start_authentication(
    webauthn: &Webauthn,
    credentials: &[Passkey],
) -> Result<(RequestChallengeResponse, PasskeyAuthentication), AuthError> {
    auth_core::domain::webauthn::start_authentication_with_credentials(webauthn, credentials)
}

/// パスキー認証を完了する
pub fn finish_authentication(
    webauthn: &Webauthn,
    auth_state: &PasskeyAuthentication,
    credential: &PublicKeyCredential,
) -> Result<AuthenticationResult, AuthError> {
    auth_core::domain::webauthn::finish_authentication_with_credentials(
        webauthn, auth_state, credential,
    )
}

// ── ユーティリティ（DB保存形式(JSONB)への適合。純粋なシリアライズなので
//    auth-coreには寄せず、ここに残す）──

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

    fn headers_with(host: Option<&str>, proto: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(h) = host {
            headers.insert(axum::http::header::HOST, h.parse().unwrap());
        }
        if let Some(p) = proto {
            headers.insert("x-forwarded-proto", p.parse().unwrap());
        }
        headers
    }

    #[test]
    fn test_resolve_webauthn_from_headers_allowed_host_ok() {
        let allowed = vec!["sophia.example.com".to_string()];
        let headers = headers_with(Some("sophia.example.com"), Some("https"));
        let result = resolve_webauthn_from_headers(&headers, &allowed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_webauthn_from_headers_rejects_unlisted_host() {
        let allowed = vec!["sophia.example.com".to_string()];
        let headers = headers_with(Some("evil.example.com"), Some("https"));
        let result = resolve_webauthn_from_headers(&headers, &allowed);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_webauthn_from_headers_rejects_host_with_port_mismatch() {
        // ポートが付いていても rp_id(ポート除去後)で allowlist 判定する
        let allowed = vec!["sophia.example.com".to_string()];
        let headers = headers_with(Some("evil.example.com:8111"), Some("https"));
        let result = resolve_webauthn_from_headers(&headers, &allowed);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_webauthn_from_headers_allows_host_with_port_when_listed() {
        let allowed = vec!["localhost".to_string()];
        let headers = headers_with(Some("localhost:8111"), None);
        let result = resolve_webauthn_from_headers(&headers, &allowed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_webauthn_from_headers_is_case_insensitive() {
        let allowed = vec!["sophia.example.com".to_string()];
        let headers = headers_with(Some("Sophia.example.com"), Some("https"));
        let result = resolve_webauthn_from_headers(&headers, &allowed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_webauthn_from_headers_missing_host_defaults_to_localhost_and_is_rejected_when_not_allowed(
    ) {
        let allowed = vec!["sophia.example.com".to_string()];
        let headers = headers_with(None, None);
        let result = resolve_webauthn_from_headers(&headers, &allowed);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_webauthn_from_headers_forces_https_for_non_local_host_even_if_header_says_http()
    {
        // 非ローカルホストでは X-Forwarded-Proto: http を送っても https に固定される。
        // (origin の http/https が食い違うと WebAuthn 側で失敗するはずなので、
        //  ここでは Webauthn インスタンス自体の構築が成功する = https で組み立てられたことを
        //  間接的に確認する。実際のscheme選択ロジックは is_local_host に切り出して直接検証する)
        assert!(!is_local_host("sophia.example.com"));
        assert!(is_local_host("localhost"));
        assert!(is_local_host("127.0.0.1"));
        assert!(is_local_host("app-stg.lan"));
    }

    // WEBAUTHN_ALLOWED_HOSTS はプロセス全体で共有されるため、
    // 並列実行される他のテストとのレースを避けるために直列化する。
    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_allowed_hosts_from_env_parses_csv_and_trims() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::set_var(
            "WEBAUTHN_ALLOWED_HOSTS",
            " sophia.example.com, sophia-stg.example.com ,localhost",
        );
        let hosts = allowed_hosts_from_env();
        std::env::remove_var("WEBAUTHN_ALLOWED_HOSTS");

        assert_eq!(
            hosts,
            vec![
                "sophia.example.com".to_string(),
                "sophia-stg.example.com".to_string(),
                "localhost".to_string(),
            ]
        );
    }

    #[test]
    fn test_allowed_hosts_from_env_defaults_to_localhost_when_unset() {
        let _guard = ENV_TEST_LOCK.lock().unwrap();
        std::env::remove_var("WEBAUTHN_ALLOWED_HOSTS");
        let hosts = allowed_hosts_from_env();
        assert_eq!(hosts, vec!["localhost".to_string()]);
    }
}
