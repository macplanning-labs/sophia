//! 実際に JWKS を返す小さなサーバーを立て、Cloudflare Access と同じ形の JWT で検証する。

use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use cf_access_jwt::{
    access_token, CfAccessUser, Config, ConfigError, Principal, TokenSourceError, Verifier,
    VerifyError, HEADER_NAME,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tower::ServiceExt;

const TEAM: &str = "https://example.cloudflareaccess.com";
const AUD: &str = "aud-tag-1";

struct TestKey {
    kid: String,
    pem: String,
    jwk: Value,
}

fn make_key(kid: &str) -> TestKey {
    let mut rng = rand::thread_rng();
    let key = RsaPrivateKey::new(&mut rng, 2048).expect("鍵生成");
    let pem = key.to_pkcs1_pem(Default::default()).unwrap().to_string();
    let jwk = json!({
        "kid": kid, "kty": "RSA", "alg": "RS256", "use": "sig",
        "n": URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()),
        "e": URL_SAFE_NO_PAD.encode(key.e().to_bytes_be()),
    });
    TestKey {
        kid: kid.into(),
        pem,
        jwk,
    }
}

fn key_a() -> &'static TestKey {
    static K: OnceLock<TestKey> = OnceLock::new();
    K.get_or_init(|| make_key("kid-a"))
}

fn key_b() -> &'static TestKey {
    static K: OnceLock<TestKey> = OnceLock::new();
    K.get_or_init(|| make_key("kid-b"))
}

/// JWKS を返すサーバー。返す鍵は途中で差し替えられる(ローテーションの再現)。
async fn jwks_server(
    keys: Vec<&'static TestKey>,
) -> (
    String,
    Arc<RwLock<Vec<Value>>>,
    Arc<std::sync::atomic::AtomicUsize>,
) {
    let state = Arc::new(RwLock::new(
        keys.iter().map(|k| k.jwk.clone()).collect::<Vec<_>>(),
    ));
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (s, h) = (state.clone(), hits.clone());
    let app = Router::new().route(
        "/cdn-cgi/access/certs",
        get(move || {
            let (s, h) = (s.clone(), h.clone());
            async move {
                h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                axum::Json(
                    json!({ "keys": *s.read().await, "public_cert": {}, "public_certs": [] }),
                )
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/cdn-cgi/access/certs"), state, hits)
}

fn config(jwks_url: &str) -> Config {
    let mut cfg = Config::new(TEAM, vec![AUD.into()]).unwrap();
    cfg.jwks_url = Some(jwks_url.into());
    cfg.allowed_email_domains = vec!["example.com".into()];
    cfg
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn user_claims() -> Value {
    json!({
        "aud": [AUD], "iss": TEAM, "email": "Taro.Yamada@Example.com",
        "sub": "user-sub-1", "type": "app", "iat": now(), "nbf": now(), "exp": now() + 3600,
    })
}

fn sign(key: &TestKey, claims: &Value) -> String {
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(key.kid.clone());
    encode(
        &header,
        claims,
        &EncodingKey::from_rsa_pem(key.pem.as_bytes()).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn 正しい_jwt_は通り_email_は小文字になる() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let p = v.verify(&sign(key_a(), &user_claims())).await.unwrap();
    assert_eq!(
        p,
        Principal::User {
            email: "taro.yamada@example.com".into(),
            sub: "user-sub-1".into()
        }
    );
}

#[tokio::test]
async fn aud_が違えば拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let mut c = user_claims();
    c["aud"] = json!(["other-app"]);
    assert!(matches!(
        v.verify(&sign(key_a(), &c)).await,
        Err(VerifyError::Invalid(_))
    ));
}

#[tokio::test]
async fn 発行者が違えば拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let mut c = user_claims();
    c["iss"] = json!("https://evil.cloudflareaccess.com");
    assert!(matches!(
        v.verify(&sign(key_a(), &c)).await,
        Err(VerifyError::Invalid(_))
    ));
}

#[tokio::test]
async fn 期限切れは拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let mut c = user_claims();
    c["exp"] = json!(now() - 600);
    assert!(matches!(
        v.verify(&sign(key_a(), &c)).await,
        Err(VerifyError::Invalid(_))
    ));
}

#[tokio::test]
async fn hs256_など_rs256_以外は拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let mut header = Header::new(jsonwebtoken::Algorithm::HS256);
    header.kid = Some("kid-a".into());
    let token = encode(&header, &user_claims(), &EncodingKey::from_secret(b"guess")).unwrap();
    assert!(matches!(
        v.verify(&token).await,
        Err(VerifyError::UnsupportedAlgorithm)
    ));
}

#[tokio::test]
async fn 公開されていない鍵で署名されたものは拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    // kid は本物と同じにして、別の鍵で署名する(鍵の取り違えを狙うなりすまし)
    let forged = TestKey {
        kid: "kid-a".into(),
        pem: key_b().pem.clone(),
        jwk: Value::Null,
    };
    assert!(matches!(
        v.verify(&sign(&forged, &user_claims())).await,
        Err(VerifyError::Invalid(_))
    ));
}

#[tokio::test]
async fn 許可していないメールドメインは拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let mut c = user_claims();
    c["email"] = json!("someone@gmail.com");
    assert!(matches!(
        v.verify(&sign(key_a(), &c)).await,
        Err(VerifyError::EmailDomainNotAllowed)
    ));
}

#[tokio::test]
async fn service_token_は既定で拒否し_許可すれば通る() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let c = json!({
        "aud": [AUD], "iss": TEAM, "common_name": "abc.access", "sub": "",
        "iat": now(), "exp": now() + 3600,
    });
    let token = sign(key_a(), &c);

    let v = Verifier::new(config(&url));
    assert!(matches!(
        v.verify(&token).await,
        Err(VerifyError::ServiceTokenNotAllowed)
    ));

    let mut cfg = config(&url);
    cfg.allow_service_tokens = true;
    let v = Verifier::new(cfg);
    assert_eq!(
        v.verify(&token).await.unwrap(),
        Principal::ServiceToken {
            common_name: "abc.access".into()
        }
    );
}

#[tokio::test]
async fn email_も_common_name_も無ければ拒否() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url));
    let c = json!({ "aud": [AUD], "iss": TEAM, "sub": "x", "exp": now() + 3600 });
    assert!(matches!(
        v.verify(&sign(key_a(), &c)).await,
        Err(VerifyError::NoIdentity)
    ));
}

#[tokio::test]
async fn 鍵のローテーション後も新しい鍵で検証できる() {
    let (url, state, _) = jwks_server(vec![key_a()]).await;
    let mut cfg = config(&url);
    cfg.jwks_min_refresh = Duration::from_millis(0);
    let v = Verifier::new(cfg);
    v.verify(&sign(key_a(), &user_claims())).await.unwrap();

    *state.write().await = vec![key_b().jwk.clone()];
    v.verify(&sign(key_b(), &user_claims())).await.unwrap();
}

#[tokio::test]
async fn 未知の_kid_が連打されても公開鍵を取りに行き過ぎない() {
    let (url, _, hits) = jwks_server(vec![key_a()]).await;
    let v = Verifier::new(config(&url)); // 最短 30 秒
    v.verify(&sign(key_a(), &user_claims())).await.unwrap();
    for _ in 0..20 {
        let r = v.verify(&sign(key_b(), &user_claims())).await;
        assert!(matches!(r, Err(VerifyError::UnknownKey)));
    }
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[tokio::test]
async fn extractor_は_jwt_無しを_403_にし_正しければ_email_を渡す() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let verifier = Arc::new(Verifier::new(config(&url)));
    let app = Router::new()
        .route("/me", get(|u: CfAccessUser| async move { u.email }))
        .with_state(verifier);

    let res = app
        .clone()
        .oneshot(Request::get("/me").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    let res = app
        .oneshot(
            Request::get("/me")
                .header(HEADER_NAME, sign(key_a(), &user_claims()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"taro.yamada@example.com");
}

#[test]
fn team_domain_は_https_必須() {
    assert!(Config::new("http://example.cloudflareaccess.com", vec![AUD.into()]).is_err());
    assert!(Config::new(TEAM, vec![]).is_err());
    assert_eq!(
        Config::new(format!("{TEAM}/"), vec![AUD.into()])
            .unwrap()
            .team_domain,
        TEAM
    );
}

#[tokio::test]
async fn cookie_の_jwt_も_ヘッダーと同じ検証を通る() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let v = Verifier::try_new(config(&url)).unwrap();
    let origins = vec!["https://app.example.com".to_string()];

    // 正しい JWT を Cookie で送り、Origin が自分のオリジンなら、取り出して検証を通る。
    let jwt = sign(key_a(), &user_claims());
    let mut h = axum::http::HeaderMap::new();
    h.insert("cookie", format!("CF_Authorization={jwt}").parse().unwrap());
    h.insert("origin", "https://app.example.com".parse().unwrap());
    let token = access_token(&h, &origins).unwrap();
    assert!(matches!(
        v.verify(token).await.unwrap(),
        Principal::User { .. }
    ));

    // 期限切れの JWT は、Cookie で来ても拒否する(取り出せるだけで信用しない)。
    let mut expired = user_claims();
    expired["exp"] = json!(now() - 3600);
    expired["nbf"] = json!(now() - 7200);
    let mut h = axum::http::HeaderMap::new();
    h.insert(
        "cookie",
        format!("CF_Authorization={}", sign(key_a(), &expired))
            .parse()
            .unwrap(),
    );
    h.insert("origin", "https://app.example.com".parse().unwrap());
    let token = access_token(&h, &origins).unwrap();
    assert!(v.verify(token).await.is_err());

    // 正しい JWT でも、別のサイトの Origin からの Cookie は取り出さない。
    let mut h = axum::http::HeaderMap::new();
    h.insert("cookie", format!("CF_Authorization={jwt}").parse().unwrap());
    h.insert("origin", "https://evil.example".parse().unwrap());
    assert_eq!(
        access_token(&h, &origins),
        Err(TokenSourceError::CookieOriginNotAllowed)
    );
}

#[test]
fn 人のログイン用の設定は_ドメイン制限が空ならエラー() {
    let cfg = Config::new(TEAM, vec![AUD.into()]).unwrap();
    assert!(cfg.allowed_email_domains.is_empty());
    match cfg.require_human_login() {
        Err(ConfigError::Missing(name)) => assert_eq!(name, "CF_ACCESS_ALLOWED_EMAIL_DOMAINS"),
        other => panic!("エラーになるはず: {other:?}"),
    }
}

#[test]
fn 人のログイン用の設定は_service_token_を必ず拒否にする() {
    let mut cfg = Config::new(TEAM, vec![AUD.into()]).unwrap();
    cfg.allowed_email_domains = vec!["example.com".into()];
    cfg.allow_service_tokens = true;
    let cfg = cfg.require_human_login().unwrap();
    assert!(!cfg.allow_service_tokens);
    assert_eq!(cfg.allowed_email_domains, vec!["example.com".to_string()]);
}

#[tokio::test]
async fn 人のログイン用の設定の検証器は_service_token_を通さない() {
    let (url, _, _) = jwks_server(vec![key_a()]).await;
    let mut cfg = config(&url);
    cfg.allow_service_tokens = true;
    let v = Verifier::try_new(cfg.require_human_login().unwrap()).unwrap();
    let claims = json!({
        "aud": [AUD], "iss": TEAM, "common_name": "client-id.access", "sub": "",
        "iat": now(), "nbf": now(), "exp": now() + 3600,
    });
    assert!(matches!(
        v.verify(&sign(key_a(), &claims)).await,
        Err(VerifyError::ServiceTokenNotAllowed)
    ));
}

#[test]
fn try_new_は_検証器を作れる() {
    let cfg = Config::new(TEAM, vec![AUD.into()]).unwrap();
    assert!(Verifier::try_new(cfg).is_ok());
}
