//! An MCP server's OAuth access token renewed (ADR-171): the authorizations
//! whose token is about to expire, each read under the lock of its row for
//! its renewal (another renewal of it waits, then reads it renewed), and
//! what a renewal brings kept, a new version of the connector.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::connector_authorizations::credential_changed;
use super::secrets::StoredSecret;

/// The connectors whose access token expires within `within`, and that
/// can be renewed.
pub async fn connector_authorizations_due(
    conn: &mut PgConnection,
    within: std::time::Duration,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "select connector_id from connector_authorizations where refresh_ciphertext is not null \
         and access_expires_at < now() + make_interval(secs => $1) order by connector_id",
    )
    .bind(within.as_secs_f64())
    .fetch_all(conn)
    .await
}

/// What a renewal reads of an authorization, its row locked until the
/// transaction ends: another renewal of it waits, then reads it renewed.
pub struct HeldRenewal {
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub access_expires_at: Option<DateTime<Utc>>,
    pub refresh: StoredSecret,
}

#[derive(sqlx::FromRow)]
struct HeldRow {
    issuer: String,
    client_id: String,
    resource: String,
    access_expires_at: Option<DateTime<Utc>>,
    refresh_ciphertext: Option<Vec<u8>>,
    refresh_iv: Option<Vec<u8>>,
    refresh_auth_tag: Option<Vec<u8>>,
    refresh_key_version: Option<i32>,
}

/// The connector's authorization, locked for its renewal, if it has one
/// that can be renewed.
pub async fn hold_connector_renewal(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<Option<HeldRenewal>> {
    let row: Option<HeldRow> = sqlx::query_as(
        "select issuer, client_id, resource, access_expires_at, refresh_ciphertext, \
         refresh_iv, refresh_auth_tag, refresh_key_version from connector_authorizations \
         where connector_id = $1 for update",
    )
    .bind(connector_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|row| {
        let refresh = StoredSecret {
            ciphertext: row.refresh_ciphertext?,
            iv: row.refresh_iv?,
            auth_tag: row.refresh_auth_tag?,
            key_version: row.refresh_key_version?,
        };
        Some(HeldRenewal {
            issuer: row.issuer,
            client_id: row.client_id,
            resource: row.resource,
            access_expires_at: row.access_expires_at,
            refresh,
        })
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

/// Keeps a renewal of the connector's authorization: a new version of the
/// connector.
pub async fn renew_connector_authorization(
    conn: &mut PgConnection,
    connector_id: Uuid,
    renewed: Renewed<'_>,
) -> sqlx::Result<()> {
    let refresh = renewed.refresh_token;
    sqlx::query(
        "update connector_authorizations set access_ciphertext = $2, access_iv = $3, \
         access_auth_tag = $4, access_key_version = $5, access_expires_at = $6, \
         refresh_ciphertext = coalesce($7, refresh_ciphertext), \
         refresh_iv = coalesce($8, refresh_iv), \
         refresh_auth_tag = coalesce($9, refresh_auth_tag), \
         refresh_key_version = coalesce($10, refresh_key_version), \
         scopes = coalesce($11, scopes), refreshed_at = now() where connector_id = $1",
    )
    .bind(connector_id)
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
    .execute(&mut *conn)
    .await?;
    credential_changed(conn, connector_id).await
}
