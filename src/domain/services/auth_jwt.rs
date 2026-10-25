/// domain/services/auth_jwt.rs — JWT クレーム発行・検証サービス
///
/// 職員/管理者向けアクセストークン（24h）と MFA検証待ち状態トークン（5分）の
/// クレーム生成、署名・検証を担当。
/// Phase 1: 認証基盤構築の一部。
///
/// ## 設計
/// - トークンタイプ識別（access / mfa_pending）により、用途外トークンの使用を防止
/// - JTI（JWT ID）を含め、ブラックリスト無効化に対応可能（後フェーズ）
/// - auth-core の JWT エンジンを薄くラップし、Sophia固有クレーム形を透過的に扱う
use auth_core::domain::jwt::{
    decode_claims as auth_decode_claims, encode_claims as auth_encode_claims,
    issue_access_claims as auth_issue_access_claims, Claims, TokenPolicy,
};
use auth_core::error::AuthError;
use chrono::Duration;
use serde::{Deserialize, Serialize};

// ── 定数 ──

pub const JWT_ISS: &str = "sophia";
pub const TOKEN_TYPE_ACCESS: &str = "access";
pub const TOKEN_TYPE_MFA_PENDING: &str = "mfa_pending";
pub const TOKEN_TYPE_REFRESH: &str = "refresh";

/// ログイン方式の値。社内 SSO（Cloudflare Access の JWT）でログインしたときだけ付く。
/// パスワード・パスキー・マジックリンクのトークンには付けない（項目なし）。
pub const AUTH_METHOD_SSO: &str = "sso";

/// デフォルトポリシー（access 30分 / refresh 7日）を返す
pub fn sophia_token_policy() -> TokenPolicy {
    TokenPolicy::default()
}

/// 30分アクセストークンの有効期限を返す
fn access_ttl() -> Duration {
    sophia_token_policy().access_ttl
}

/// 7日リフレッシュトークンの有効期限を返す
fn refresh_ttl() -> Duration {
    sophia_token_policy().refresh_ttl
}

/// 5分 MFA検証待ちトークンの有効期限を返す
fn mfa_pending_ttl() -> Duration {
    Duration::minutes(5)
}

// ── Sophia固有のextraクレーム ──

/// JWT.extra に埋め込まれる Sophia 固有クレーム
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SophiaExtraClaims {
    /// ロール："ADMIN" | "EMPLOYEE" | "USER"（呼び出し元で検証）
    pub role: String,
    /// MFA検証済みか（access token は true, mfa_pending は false）
    pub mfa_verified: bool,
    /// JWT ID（ブラックリスト無効化用）
    pub jti: String,
    /// トークンタイプ（TOKEN_TYPE_ACCESS / TOKEN_TYPE_MFA_PENDING / TOKEN_TYPE_REFRESH）
    pub token_type: String,
    /// ログイン方式。社内 SSO のときだけ Some("sso")。
    /// 項目が無い古いトークンも読める（None）。サイレントリフレッシュでも引き継ぐ（欠けると 30 分後に
    /// MFA の登録を求められてしまう）。SSO は IdP が本人確認を済ませているため、Sophia の MFA の登録を免除する
    /// （DEMO-000226 決定 6。免除はこの項目が付いたトークンだけ。DB の mfa_enabled は変えない）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
}

impl SophiaExtraClaims {
    /// 社内 SSO でログインしたトークンか。
    pub fn is_sso(&self) -> bool {
        self.auth_method.as_deref() == Some(AUTH_METHOD_SSO)
    }
}

/// ログイン成功時に発行される access + refresh トークンペア
#[derive(Debug)]
pub struct StaffTokenPair {
    pub access: String,
    pub refresh: String,
    pub access_jti: String,
    pub refresh_jti: String,
}

// ── クレーム発行 ──

/// staff/admin用アクセストークン（30分）のClaimsを組み立てる。
/// jtiも生成して返す（呼び出し元がCookie発行と同時にblacklist登録用にjtiを保持できるようにするため）。
pub fn issue_sophia_access_claims(
    user_id: i64,
    role: &str,
    mfa_verified: bool,
) -> Result<(Claims, String), AuthError> {
    issue_sophia_access_claims_with_method(user_id, role, mfa_verified, None)
}

/// [`issue_sophia_access_claims`] に、ログイン方式（SSO のときだけ Some(AUTH_METHOD_SSO)）を足した版。
pub fn issue_sophia_access_claims_with_method(
    user_id: i64,
    role: &str,
    mfa_verified: bool,
    auth_method: Option<&str>,
) -> Result<(Claims, String), AuthError> {
    let jti = uuid::Uuid::new_v4().to_string();
    let extra = SophiaExtraClaims {
        role: role.to_string(),
        mfa_verified,
        jti: jti.clone(),
        token_type: TOKEN_TYPE_ACCESS.to_string(),
        auth_method: auth_method.map(str::to_string),
    };
    let policy = TokenPolicy {
        access_ttl: access_ttl(),
        refresh_ttl: refresh_ttl(),
    };
    let extra_value = serde_json::to_value(&extra)
        .map_err(|e| AuthError::Internal(format!("claims のシリアライズに失敗: {e}")))?;
    let claims = auth_issue_access_claims(&user_id.to_string(), &[], JWT_ISS, policy, extra_value);
    Ok((claims, jti))
}

/// リフレッシュトークン（7日）のClaimsを組み立てる。
/// token_type は TOKEN_TYPE_REFRESH。mfa_verified は常に true（本ログイン後のみ）。
pub fn issue_sophia_refresh_claims(user_id: i64, role: &str) -> Result<(Claims, String), AuthError> {
    issue_sophia_refresh_claims_with_method(user_id, role, None)
}

/// [`issue_sophia_refresh_claims`] に、ログイン方式を足した版。
pub fn issue_sophia_refresh_claims_with_method(
    user_id: i64,
    role: &str,
    auth_method: Option<&str>,
) -> Result<(Claims, String), AuthError> {
    let jti = uuid::Uuid::new_v4().to_string();
    let extra = SophiaExtraClaims {
        role: role.to_string(),
        mfa_verified: true,
        jti: jti.clone(),
        token_type: TOKEN_TYPE_REFRESH.to_string(),
        auth_method: auth_method.map(str::to_string),
    };
    let policy = TokenPolicy {
        access_ttl: refresh_ttl(),
        refresh_ttl: refresh_ttl(),
    };
    let extra_value = serde_json::to_value(&extra)
        .map_err(|e| AuthError::Internal(format!("claims のシリアライズに失敗: {e}")))?;
    let claims = auth_issue_access_claims(&user_id.to_string(), &[], JWT_ISS, policy, extra_value);
    Ok((claims, jti))
}

/// MFA未検証状態を表す短命（5分）トークンのClaimsを組み立てる。
pub fn issue_mfa_pending_claims(user_id: i64, role: &str) -> Result<Claims, AuthError> {
    let jti = uuid::Uuid::new_v4().to_string();
    let extra = SophiaExtraClaims {
        role: role.to_string(),
        mfa_verified: false,
        jti,
        token_type: TOKEN_TYPE_MFA_PENDING.to_string(),
        auth_method: None,
    };
    let policy = TokenPolicy {
        access_ttl: mfa_pending_ttl(),
        refresh_ttl: mfa_pending_ttl(),
    };
    let extra_value = serde_json::to_value(&extra)
        .map_err(|e| AuthError::Internal(format!("claims のシリアライズに失敗: {e}")))?;
    Ok(auth_issue_access_claims(&user_id.to_string(), &[], JWT_ISS, policy, extra_value))
}

// ── ペア発行 ──

/// MFA完了後の access + refresh トークンペアを発行してエンコードする。
pub fn issue_and_encode_token_pair(
    user_id: i64,
    role: &str,
    secret: &str,
) -> Result<StaffTokenPair, AuthError> {
    issue_and_encode_token_pair_with_method(user_id, role, secret, None)
}

/// [`issue_and_encode_token_pair`] に、ログイン方式を足した版。
/// access と refresh の**両方**に同じ方式を入れる（片方だけだと、サイレントリフレッシュで方式が消える）。
pub fn issue_and_encode_token_pair_with_method(
    user_id: i64,
    role: &str,
    secret: &str,
    auth_method: Option<&str>,
) -> Result<StaffTokenPair, AuthError> {
    let (access_claims, access_jti) =
        issue_sophia_access_claims_with_method(user_id, role, true, auth_method)?;
    let access = encode_sophia_claims(&access_claims, secret)?;

    let (refresh_claims, refresh_jti) =
        issue_sophia_refresh_claims_with_method(user_id, role, auth_method)?;
    let refresh = encode_sophia_claims(&refresh_claims, secret)?;

    Ok(StaffTokenPair {
        access,
        refresh,
        access_jti,
        refresh_jti,
    })
}

/// サイレントリフレッシュ用。access トークンだけを再発行。
/// refresh の extra（role / mfa_verified）をコピーして使う。
pub fn issue_and_encode_access(
    user_id: i64,
    role: &str,
    mfa_verified: bool,
    secret: &str,
) -> Result<String, AuthError> {
    issue_and_encode_access_with_method(user_id, role, mfa_verified, secret, None)
}

/// [`issue_and_encode_access`] に、ログイン方式（refresh の extra から引き継ぐ値）を足した版。
pub fn issue_and_encode_access_with_method(
    user_id: i64,
    role: &str,
    mfa_verified: bool,
    secret: &str,
    auth_method: Option<&str>,
) -> Result<String, AuthError> {
    let (claims, _) =
        issue_sophia_access_claims_with_method(user_id, role, mfa_verified, auth_method)?;
    encode_sophia_claims(&claims, secret)
}

// ── エンコード・デコード ──

/// Claimsを秘密鍵で署名しJWT文字列にする（auth_core::domain::jwt::encode_claimsの薄いラッパー）。
pub fn encode_sophia_claims(claims: &Claims, secret: &str) -> Result<String, AuthError> {
    auth_encode_claims(claims, secret)
}

/// JWT文字列を検証し、Claims本体とSophia固有のextraクレームに分解する。
pub fn decode_sophia_claims(
    token: &str,
    secret: &str,
) -> Result<(Claims, SophiaExtraClaims), AuthError> {
    let claims: Claims = auth_decode_claims(token, secret)?;
    let extra: SophiaExtraClaims = serde_json::from_value(claims.extra.clone())
        .map_err(|e| AuthError::InvalidToken(e.to_string()))?;
    Ok((claims, extra))
}

// ── テスト ──

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-key";

    #[test]
    fn test_issue_and_decode_access_claims_roundtrip() {
        let user_id = 42i64;
        let role = "ADMIN";
        let mfa_verified = true;

        let (claims, jti) = issue_sophia_access_claims(user_id, role, mfa_verified).unwrap();
        let token = encode_sophia_claims(&claims, TEST_SECRET).unwrap();
        let (decoded_claims, decoded_extra) = decode_sophia_claims(&token, TEST_SECRET).unwrap();

        assert_eq!(decoded_claims.sub, "42");
        assert_eq!(decoded_extra.role, "ADMIN");
        assert!(decoded_extra.mfa_verified);
        assert_eq!(decoded_extra.token_type, TOKEN_TYPE_ACCESS);
        assert_eq!(decoded_extra.jti, jti);

        // access TTL 検証（30分 = 1800秒）
        let ttl_secs = decoded_claims.exp - decoded_claims.iat;
        assert!(1700 <= ttl_secs && ttl_secs <= 1900, "access TTL {} outside range", ttl_secs);
    }

    #[test]
    fn test_issue_and_decode_refresh_claims() {
        let user_id = 42i64;
        let role = "ADMIN";

        let (claims, jti) = issue_sophia_refresh_claims(user_id, role).unwrap();
        let token = encode_sophia_claims(&claims, TEST_SECRET).unwrap();
        let (decoded_claims, decoded_extra) = decode_sophia_claims(&token, TEST_SECRET).unwrap();

        assert_eq!(decoded_claims.sub, "42");
        assert_eq!(decoded_extra.role, "ADMIN");
        assert!(decoded_extra.mfa_verified);
        assert_eq!(decoded_extra.token_type, TOKEN_TYPE_REFRESH);
        assert_eq!(decoded_extra.jti, jti);

        // refresh TTL 検証（7日 = 604800秒）
        let ttl_secs = decoded_claims.exp - decoded_claims.iat;
        let expected_refresh_ttl = 7 * 24 * 60 * 60;
        assert!(
            (expected_refresh_ttl - 60) <= ttl_secs && ttl_secs <= (expected_refresh_ttl + 60),
            "refresh TTL {} outside range",
            ttl_secs
        );
    }

    #[test]
    fn test_issue_and_decode_mfa_pending_claims_roundtrip() {
        let user_id = 42i64;
        let role = "ADMIN";

        let claims = issue_mfa_pending_claims(user_id, role).unwrap();
        let token = encode_sophia_claims(&claims, TEST_SECRET).unwrap();
        let (decoded_claims, decoded_extra) = decode_sophia_claims(&token, TEST_SECRET).unwrap();

        assert_eq!(decoded_claims.sub, "42");
        assert_eq!(decoded_extra.role, "ADMIN");
        assert!(!decoded_extra.mfa_verified);
        assert_eq!(decoded_extra.token_type, TOKEN_TYPE_MFA_PENDING);
    }

    // ── ログイン方式（社内 SSO）── DEMO-000233 / 決定 6

    #[test]
    fn auth_method_は_通常のログインでは付かない() {
        let pair = issue_and_encode_token_pair(7, "EMPLOYEE", TEST_SECRET).unwrap();
        for tok in [&pair.access, &pair.refresh] {
            let (_, extra) = decode_sophia_claims(tok, TEST_SECRET).unwrap();
            assert_eq!(extra.auth_method, None);
            assert!(!extra.is_sso());
        }
    }

    #[test]
    fn sso_のログインでは_access_と_refresh_の両方に方式が付く() {
        let pair =
            issue_and_encode_token_pair_with_method(7, "EMPLOYEE", TEST_SECRET, Some(AUTH_METHOD_SSO))
                .unwrap();
        let (_, a) = decode_sophia_claims(&pair.access, TEST_SECRET).unwrap();
        let (_, r) = decode_sophia_claims(&pair.refresh, TEST_SECRET).unwrap();
        assert!(a.is_sso(), "access に方式が付く");
        assert!(r.is_sso(), "refresh に方式が付く(欠けるとサイレントリフレッシュで消える)");
        assert_eq!(a.token_type, TOKEN_TYPE_ACCESS);
        assert_eq!(r.token_type, TOKEN_TYPE_REFRESH);
        assert!(a.mfa_verified && r.mfa_verified);
    }

    #[test]
    fn リフレッシュで_access_を作り直しても_方式が引き継がれる() {
        let access = issue_and_encode_access_with_method(
            7,
            "ADMIN",
            true,
            TEST_SECRET,
            Some(AUTH_METHOD_SSO),
        )
        .unwrap();
        let (_, extra) = decode_sophia_claims(&access, TEST_SECRET).unwrap();
        assert!(extra.is_sso());
        // 方式なしで作り直せば付かない(既存の呼び出しの挙動は変わらない)
        let plain = issue_and_encode_access(7, "ADMIN", true, TEST_SECRET).unwrap();
        let (_, extra) = decode_sophia_claims(&plain, TEST_SECRET).unwrap();
        assert!(!extra.is_sso());
    }

    #[test]
    fn 方式の項目が無い古いトークンも読める() {
        // auth_method が導入される前に発行されたトークン(項目なし)。読めないと、リリース直後に全員がログアウトになる。
        let old = serde_json::json!({
            "role": "ADMIN", "mfa_verified": true, "jti": "j", "token_type": "access"
        });
        let extra: SophiaExtraClaims = serde_json::from_value(old).unwrap();
        assert_eq!(extra.auth_method, None);
        // 通常のトークンは、項目そのものを出力しない(古いコードが読んでも壊れない)
        let v = serde_json::to_value(&extra).unwrap();
        assert!(v.get("auth_method").is_none());
    }

    #[test]
    fn sso_以外の値は_sso_として扱わない() {
        let extra = SophiaExtraClaims {
            role: "ADMIN".into(),
            mfa_verified: true,
            jti: "j".into(),
            token_type: TOKEN_TYPE_ACCESS.into(),
            auth_method: Some("password".into()),
        };
        assert!(!extra.is_sso());
    }

    #[test]
    fn test_expired_token_decoding_fails() {
        let now = chrono::Utc::now();
        let past_time = (now - Duration::minutes(5)).timestamp();

        let claims = Claims {
            sub: "42".to_string(),
            roles: vec![],
            exp: past_time,
            iat: now.timestamp(),
            iss: JWT_ISS.to_string(),
            extra: serde_json::to_value(SophiaExtraClaims {
                role: "ADMIN".to_string(),
                mfa_verified: true,
                jti: uuid::Uuid::new_v4().to_string(),
                token_type: TOKEN_TYPE_ACCESS.to_string(),
                auth_method: None,
            })
            .unwrap(),
        };

        let token = encode_sophia_claims(&claims, TEST_SECRET).unwrap();
        let result = decode_sophia_claims(&token, TEST_SECRET);

        assert!(matches!(result, Err(AuthError::TokenExpired)));
    }
}
