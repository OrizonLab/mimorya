//! The tokens an authorization server grants an MCP server's authorization,
//! sealed for its connector (ADR-171): by the authorization's return and by
//! its renewal alike. An expiry is bounded; scopes are kept as the server
//! said them, if it did.

use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::mcp::oauth::Granted;
use mimorya_db::repositories::secrets::StoredSecret;
use uuid::Uuid;

use super::{
    access_sealed_as, person_access_sealed_as, person_refresh_sealed_as, refresh_sealed_as,
};
use crate::secrets::SecretCipher;

/// What an authorization's removal is audited as: by an administrator, or
/// by the system when the server no longer honours its refresh token.
pub const AUTHORIZATION_REMOVED: &str = "connector.oauth_authorization_removed";

/// An expiry further than this is read as this.
const MAX_LIFETIME_SECONDS: u64 = 10 * 365 * 24 * 3600;

/// Granted tokens, sealed for the connector.
pub struct SealedGrant {
    pub access: StoredSecret,
    /// A refresh token, if one was given.
    pub refresh: Option<StoredSecret>,
    pub expires_at: Option<DateTime<Utc>>,
    /// The scopes granted, as one string, if the server said them.
    pub scopes: Option<String>,
}

/// `granted`, sealed for the connector `connector_id` of `tenant_id`.
pub fn seal_grant(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    connector_id: Uuid,
    granted: &Granted,
) -> anyhow::Result<SealedGrant> {
    let names = (
        access_sealed_as(connector_id),
        refresh_sealed_as(connector_id),
    );
    seal_as(cipher, tenant_id, names, granted)
}

/// `granted`, sealed for the connector `connector_id` and the person
/// `user_id` (ADR-174): opened for that person only.
pub fn seal_person_grant(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    connector_id: Uuid,
    user_id: Uuid,
    granted: &Granted,
) -> anyhow::Result<SealedGrant> {
    let names = (
        person_access_sealed_as(connector_id, user_id),
        person_refresh_sealed_as(connector_id, user_id),
    );
    seal_as(cipher, tenant_id, names, granted)
}

fn seal_as(
    cipher: &SecretCipher,
    tenant_id: Uuid,
    (access_name, refresh_name): (String, String),
    granted: &Granted,
) -> anyhow::Result<SealedGrant> {
    let access = cipher.seal(tenant_id, &access_name, &granted.access_token)?;
    let refresh = match &granted.refresh_token {
        Some(token) => Some(cipher.seal(tenant_id, &refresh_name, token)?),
        None => None,
    };
    let expires_at = granted
        .expires_in
        .map(|seconds| Utc::now() + Duration::seconds(seconds.min(MAX_LIFETIME_SECONDS) as i64));
    Ok(SealedGrant {
        access,
        refresh,
        expires_at,
        scopes: granted.scopes.as_ref().map(|scopes| scopes.join(" ")),
    })
}
