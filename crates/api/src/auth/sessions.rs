//! Opens a session for an account whose credentials were just checked (a
//! login, an accepted invitation): only its token's digest is stored, and
//! the token itself goes back in the session cookie.

use axum::http::HeaderMap;
use axum::http::header::USER_AGENT;
use chrono::Utc;
use mimorya_db::TenantTx;
use mimorya_db::repositories::auth::{NewSession, create_session};
use uuid::Uuid;

use super::credentials::{SESSION_ABSOLUTE, SESSION_IDLE, new_session_token};
use crate::plugins::RequestContext;

/// Returns the session token, for the cookie.
pub async fn open_session(
    tx: &mut TenantTx,
    user_id: Uuid,
    context: &RequestContext,
    headers: &HeaderMap,
) -> sqlx::Result<String> {
    let (token, token_hash) = new_session_token();
    let now = Utc::now();
    let tenant_id = tx.tenant_id();
    create_session(
        tx,
        NewSession {
            tenant_id,
            user_id,
            token_hash,
            expires_at: now + SESSION_IDLE,
            absolute_expires_at: now + SESSION_ABSOLUTE,
            ip: context.ip_string(),
            user_agent: headers
                .get(USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string),
        },
    )
    .await?;
    Ok(token)
}
