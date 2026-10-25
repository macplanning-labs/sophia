//! axum の Extractor。State から `Arc<Verifier>` を取り出して検証する。
//!
//! 失敗時は理由を返さず 403 のみを返す(理由はサーバーのログにだけ残す)。

use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::StatusCode;

use crate::{Principal, Verifier, VerifyError, HEADER_NAME};

/// 人・Service Token のどちらでも通す Extractor。
/// Service Token を受け入れるかは [`crate::Config::allow_service_tokens`] で決まる。
#[derive(Debug, Clone)]
pub struct CfAccess(pub Principal);

/// 人だけを通す Extractor。email は小文字に正規化済み。
#[derive(Debug, Clone)]
pub struct CfAccessUser {
    pub email: String,
    pub sub: String,
}

async fn verify_parts(parts: &Parts, verifier: &Verifier) -> Result<Principal, StatusCode> {
    let token = parts
        .headers
        .get(HEADER_NAME)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    verifier.verify(token).await.map_err(|e| {
        match &e {
            VerifyError::Jwks(_) => tracing::error!(error = %e, "Access JWT を検証できません"),
            _ => tracing::warn!(error = %e, path = %parts.uri.path(), "Access JWT を拒否しました"),
        }
        StatusCode::FORBIDDEN
    })
}

impl<S> FromRequestParts<S> for CfAccess
where
    Arc<Verifier>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let verifier = Arc::<Verifier>::from_ref(state);
        verify_parts(parts, &verifier).await.map(CfAccess)
    }
}

impl<S> FromRequestParts<S> for CfAccessUser
where
    Arc<Verifier>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let verifier = Arc::<Verifier>::from_ref(state);
        match verify_parts(parts, &verifier).await? {
            Principal::User { email, sub } => Ok(CfAccessUser { email, sub }),
            Principal::ServiceToken { .. } => {
                tracing::warn!(path = %parts.uri.path(), "人専用の API に Service Token で来たため拒否しました");
                Err(StatusCode::FORBIDDEN)
            }
        }
    }
}
