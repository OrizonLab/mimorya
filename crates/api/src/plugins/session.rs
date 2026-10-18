//! Session resolution: the session cookie is resolved on every request into
//! an `AuthIdentity`, and the idle expiry is extended at most once per
//! interval. Routes obtain the identity through the `Auth` extractor, which
//! refuses an account holding a temporary password: only the routes that let
//! it replace that password take `AuthPendingPasswordChange` instead. Routes
//! needing no identity (login, health) are not concerned.

use std::convert::Infallible;

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use mimorya_core::DomainError;
use mimorya_db::repositories::auth::{ResolvedSession, resolve_session, touch_session};
use serde_json::json;

use crate::app::AppState;
use crate::auth::cookies::session_token;
use crate::auth::credentials::{SESSION_IDLE, SESSION_TOUCH_INTERVAL, hash_token};
use crate::http::{ApiError, ApiResult};
use crate::services::AuthIdentity;

pub async fn resolve_identity(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(token) = session_token(request.headers()) {
        match resolve(&state, &token).await {
            Ok(Some(identity)) => {
                request.extensions_mut().insert(identity);
            }
            Ok(None) => {}
            Err(error) => return error.into_response(),
        }
    }
    next.run(request).await
}

async fn resolve(state: &AppState, token: &str) -> ApiResult<Option<AuthIdentity>> {
    let Some(session) = resolve_session(state.db.pool(), &hash_token(token)).await? else {
        return Ok(None);
    };
    extend_if_due(state, &session).await?;
    Ok(Some(AuthIdentity {
        tenant_id: session.tenant_id,
        user_id: session.user_id,
        session_id: session.session_id,
        must_change_password: session.must_change_password,
    }))
}

fn password_change_required() -> DomainError {
    DomainError::forbidden("Choisissez un nouveau mot de passe pour continuer.")
        .with_details(json!({ "reason": "password_change_required" }))
}

/// Sliding expiry, bounded by the absolute expiry of the session.
async fn extend_if_due(state: &AppState, session: &ResolvedSession) -> ApiResult<()> {
    let now = Utc::now();
    let remaining = session.expires_at - now;
    if SESSION_IDLE - remaining <= SESSION_TOUCH_INTERVAL {
        return Ok(());
    }
    let expires_at = (now + SESSION_IDLE).min(session.absolute_expires_at);
    let mut tx = state.db.begin(session.tenant_id).await?;
    touch_session(&mut tx, session.session_id, expires_at).await?;
    tx.commit().await?;
    Ok(())
}

/// The signed-in user; answers 401 otherwise, and 403 while the account must
/// replace a temporary password.
pub struct Auth(pub AuthIdentity);

impl<S: Send + Sync> FromRequestParts<S> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let identity = signed_in(parts)?;
        if identity.must_change_password {
            return Err(password_change_required().into());
        }
        Ok(Self(identity))
    }
}

/// The signed-in user, even while their password is temporary: for the
/// routes that let them see who they are and replace it.
pub struct AuthPendingPasswordChange(pub AuthIdentity);

impl<S: Send + Sync> FromRequestParts<S> for AuthPendingPasswordChange {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        signed_in(parts).map(Self)
    }
}

fn signed_in(parts: &Parts) -> Result<AuthIdentity, ApiError> {
    parts
        .extensions
        .get::<AuthIdentity>()
        .copied()
        .ok_or_else(|| DomainError::unauthenticated("Authentification requise").into())
}

/// The signed-in user, if any.
pub struct MaybeAuth(pub Option<AuthIdentity>);

impl<S: Send + Sync> FromRequestParts<S> for MaybeAuth {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(parts.extensions.get::<AuthIdentity>().copied()))
    }
}
