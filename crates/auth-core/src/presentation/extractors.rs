//! presentation/extractors.rs — ジェネリックなAxum Extractor（方針ドキュメント7章）
//!
//! auth-coreはユーザーの永続化・ドメイン表現を知らない。「認証に必要な最小限の
//! 情報を取得できるアプリ側の実装」をトレイトで受け取るジェネリック設計にする。
//! WIP・Sophiaそれぞれが`AuthUserRepository`（＋`AuthConfigProvider`）をAppStateに
//! 実装することで、ハンドラは共通の文法（`AuthUser<U>`）で認証情報を扱える。

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::domain::jwt::{decode_claims, Claims};
use crate::error::AuthError;

/// アプリのユーザーリポジトリが実装するトレイト。
/// `Claims::sub`（JWTの`sub`クレーム＝資格情報ID）からアプリのUser型を引く。
#[async_trait::async_trait]
pub trait AuthUserRepository {
    type User: Send + Sync + Clone;

    async fn find_by_credentials_id(&self, id: &str) -> Result<Option<Self::User>, AuthError>;
    fn user_id(user: &Self::User) -> String;
    fn roles(user: &Self::User) -> Vec<String>;
}

/// JWT検証に必要な設定をAppStateから取得するためのトレイト。
pub trait AuthConfigProvider {
    fn jwt_secret(&self) -> &str;
}

/// 認証済みユーザーを表すExtractor。
/// `AuthUserRepository<User = U>` と `AuthConfigProvider` を実装したAppStateを
/// 持つハンドラで `AuthUser(user): AuthUser<MyUser>` のように使う。
pub struct AuthUser<U>(pub U);

impl<S, U> FromRequestParts<S> for AuthUser<U>
where
    S: AuthUserRepository<User = U> + AuthConfigProvider + Send + Sync,
    U: Send + Sync + Clone,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                AuthError::InvalidToken("Authorizationヘッダーがありません".to_string())
            })?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AuthError::InvalidToken("Bearer形式ではありません".to_string()))?;

        let claims: Claims = decode_claims(token, state.jwt_secret())?;

        let user = state
            .find_by_credentials_id(&claims.sub)
            .await?
            .ok_or(AuthError::NotFound)?;

        Ok(AuthUser(user))
    }
}
