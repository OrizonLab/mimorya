//! An MCP server's OAuth access token renewed (ADR-171), without holding a
//! lock while the token endpoint is called: the renewal is claimed for a
//! while by one statement (`renewing_until`), so that another process skips
//! it and starts with the current token, and a token endpoint that did not
//! answer is not asked again before the claim ends. What the renewal brings
//! is kept, or the authorization removed, only if it is still the one that
//! was claimed (same row, not renewed since): a new version of the
//! connector either way.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::connector_authorizations::credential_changed;
use super::secrets::StoredSecret;

/// The enabled connectors whose access token expires within `within`, that
/// can be renewed and whose renewal is not claimed: a disabled connector's
/// server is not asked.
pub async fn connector_authorizations_due(
    conn: &mut PgConnection,
    within: std::time::Duration,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "select a.connector_id from connector_authorizations a \
         join connector_instances i on i.id = a.connector_id and i.enabled \
         where a.refresh_ciphertext is not null \
         and a.access_expires_at < now() + make_interval(secs => $1) \
         and (a.renewing_until is null or a.renewing_until <= now()) order by a.connector_id",
    )
    .bind(within.as_secs_f64())
    .fetch_all(conn)
    .await
}

/// Which authorization a renewal was claimed for: kept or removed only if
/// it is still this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClaimedRenewal {
    pub authorization_id: Uuid,
    pub refreshed_at: DateTime<Utc>,
}

/// What a claimed renewal reads of the authorization.
pub struct Claimed {
    pub key: ClaimedRenewal,
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub refresh: StoredSecret,
}

#[derive(sqlx::FromRow)]
struct ClaimedRow {
    id: Uuid,
    refreshed_at: DateTime<Utc>,
    issuer: String,
    client_id: String,
    resource: String,
    refresh_ciphertext: Vec<u8>,
    refresh_iv: Vec<u8>,
    refresh_auth_tag: Vec<u8>,
    refresh_key_version: i32,
}

/// Claims the renewal of the connector's authorization for `hold`, if it is
/// due within `within`, renewable, not claimed already, and the connector
/// enabled.
pub async fn claim_connector_renewal(
    conn: &mut PgConnection,
    connector_id: Uuid,
    within: std::time::Duration,
    hold: std::time::Duration,
) -> sqlx::Result<Option<Claimed>> {
    let row: Option<ClaimedRow> = sqlx::query_as(
        "update connector_authorizations set renewing_until = now() + make_interval(secs => $3) \
         where connector_id = $1 and refresh_ciphertext is not null \
         and access_expires_at < now() + make_interval(secs => $2) \
         and (renewing_until is null or renewing_until <= now()) \
         and exists (select 1 from connector_instances i where i.id = connector_id and i.enabled) \
         returning id, refreshed_at, issuer, client_id, resource, refresh_ciphertext, \
         refresh_iv, refresh_auth_tag, refresh_key_version",
    )
    .bind(connector_id)
    .bind(within.as_secs_f64())
    .bind(hold.as_secs_f64())
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| Claimed {
        key: ClaimedRenewal {
            authorization_id: row.id,
            refreshed_at: row.refreshed_at,
        },
        issuer: row.issuer,
        client_id: row.client_id,
        resource: row.resource,
        refresh: StoredSecret {
            ciphertext: row.refresh_ciphertext,
            iv: row.refresh_iv,
            auth_tag: row.refresh_auth_tag,
            key_version: row.refresh_key_version,
        },
    }))
}

/// What a renewal brings, sealed for the connector.
pub struct Renewed<'a> {
    pub access_token: &'a StoredSecret,
    pub access_expires_at: Option<DateTime<Utc>>,
    /// A rotated refresh token; `None` keeps the one held.
    pub refresh_token: Option<&'a StoredSecret>,
    /// The scopes granted, if the server said them.
    pub scopes: Option<&'a str>,
}

/// Keeps a renewal of the authorization claimed, if it is still that one;
/// returns its connector if it was kept.
pub async fn keep_connector_renewal(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
    renewed: Renewed<'_>,
) -> sqlx::Result<Option<Uuid>> {
    let refresh = renewed.refresh_token;
    let connector: Option<Uuid> = sqlx::query_scalar(
        "update connector_authorizations set access_ciphertext = $3, access_iv = $4, \
         access_auth_tag = $5, access_key_version = $6, access_expires_at = $7, \
         refresh_ciphertext = coalesce($8, refresh_ciphertext), \
         refresh_iv = coalesce($9, refresh_iv), \
         refresh_auth_tag = coalesce($10, refresh_auth_tag), \
         refresh_key_version = coalesce($11, refresh_key_version), \
         scopes = coalesce($12, scopes), refreshed_at = now(), renewing_until = null \
         where id = $1 and refreshed_at = $2 returning connector_id",
    )
    .bind(claimed.authorization_id)
    .bind(claimed.refreshed_at)
    .bind(&renewed.access_token.ciphertext)
    .bind(&renewed.access_token.iv)
    .bind(&renewed.access_token.auth_tag)
    .bind(renewed.access_token.key_version)
    .bind(renewed.access_expires_at)
    .bind(refresh.map(|r| r.ciphertext.clone()))
    .bind(refresh.map(|r| r.iv.clone()))
    .bind(refresh.map(|r| r.auth_tag.clone()))
    .bind(refresh.map(|r| r.key_version))
    .bind(renewed.scopes)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(connector_id) = connector {
        credential_changed(conn, connector_id).await?;
    }
    Ok(connector)
}

/// Removes the authorization claimed, the server no longer honouring it, if
/// it is still that one; returns whether it was removed.
pub async fn remove_claimed_authorization(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
) -> sqlx::Result<bool> {
    let connector: Option<Uuid> = sqlx::query_scalar(
        "delete from connector_authorizations where id = $1 and refreshed_at = $2 \
         returning connector_id",
    )
    .bind(claimed.authorization_id)
    .bind(claimed.refreshed_at)
    .fetch_optional(&mut *conn)
    .await?;
    match connector {
        Some(connector_id) => {
            credential_changed(conn, connector_id).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}
