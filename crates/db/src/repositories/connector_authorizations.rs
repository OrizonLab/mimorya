//! The OAuth authorization of an MCP server (ADR-171): the tokens its
//! authorization server issued for the company, sealed by the API before
//! they reach the database, with what they were issued for, who authorized
//! and when. One per connector; a new authorization replaces the previous.
//! Keeping or removing one makes a new version of the connector: every
//! process starts its session again with the token it then reads, even one
//! that missed the invalidation.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::secrets::StoredSecret;

/// An authorization as listed: never its sealed tokens.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ConnectorAuthorizationRow {
    pub connector_id: Uuid,
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub scopes: String,
    pub access_expires_at: Option<DateTime<Utc>>,
    /// Whether a refresh token is kept.
    pub renewable: bool,
    pub authorized_by_user_id: Option<Uuid>,
    pub authorized_at: DateTime<Utc>,
    pub refreshed_at: DateTime<Utc>,
}

pub struct NewConnectorAuthorization<'a> {
    pub tenant_id: Uuid,
    pub connector_id: Uuid,
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub resource: &'a str,
    pub scopes: &'a str,
    pub access_token: &'a StoredSecret,
    pub access_expires_at: Option<DateTime<Utc>>,
    pub refresh_token: Option<&'a StoredSecret>,
    pub authorized_by_user_id: Uuid,
}

/// Keeps the connector's authorization, replacing the previous one;
/// returns whether one was replaced.
pub async fn put_connector_authorization(
    conn: &mut PgConnection,
    new: NewConnectorAuthorization<'_>,
) -> sqlx::Result<bool> {
    let replaced = delete_connector_authorization(&mut *conn, new.connector_id).await?;
    let refresh = new.refresh_token;
    sqlx::query(
        "insert into connector_authorizations (tenant_id, connector_id, issuer, client_id, \
         resource, scopes, access_ciphertext, access_iv, access_auth_tag, access_key_version, \
         access_expires_at, refresh_ciphertext, refresh_iv, refresh_auth_tag, \
         refresh_key_version, authorized_by_user_id) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(new.tenant_id)
    .bind(new.connector_id)
    .bind(new.issuer)
    .bind(new.client_id)
    .bind(new.resource)
    .bind(new.scopes)
    .bind(&new.access_token.ciphertext)
    .bind(&new.access_token.iv)
    .bind(&new.access_token.auth_tag)
    .bind(new.access_token.key_version)
    .bind(new.access_expires_at)
    .bind(refresh.map(|r| r.ciphertext.clone()))
    .bind(refresh.map(|r| r.iv.clone()))
    .bind(refresh.map(|r| r.auth_tag.clone()))
    .bind(refresh.map(|r| r.key_version))
    .bind(new.authorized_by_user_id)
    .execute(&mut *conn)
    .await?;
    credential_changed(conn, new.connector_id).await?;
    Ok(replaced)
}

/// The connector's authorization, if it has one.
pub async fn get_connector_authorization(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<Option<ConnectorAuthorizationRow>> {
    sqlx::query_as(
        "select connector_id, issuer, client_id, resource, scopes, access_expires_at, \
         refresh_ciphertext is not null as renewable, authorized_by_user_id, authorized_at, \
         refreshed_at from connector_authorizations where connector_id = $1",
    )
    .bind(connector_id)
    .fetch_optional(conn)
    .await
}

#[derive(sqlx::FromRow)]
struct SealedRow {
    access_ciphertext: Vec<u8>,
    access_iv: Vec<u8>,
    access_auth_tag: Vec<u8>,
    access_key_version: i32,
    refresh_ciphertext: Option<Vec<u8>>,
    refresh_iv: Option<Vec<u8>>,
    refresh_auth_tag: Option<Vec<u8>>,
    refresh_key_version: Option<i32>,
}

/// The connector's sealed access token and refresh token, if any.
pub async fn sealed_connector_tokens(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<Option<(StoredSecret, Option<StoredSecret>)>> {
    let row: Option<SealedRow> = sqlx::query_as(
        "select access_ciphertext, access_iv, access_auth_tag, access_key_version, \
         refresh_ciphertext, refresh_iv, refresh_auth_tag, refresh_key_version \
         from connector_authorizations where connector_id = $1",
    )
    .bind(connector_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| {
        let access = StoredSecret {
            ciphertext: row.access_ciphertext,
            iv: row.access_iv,
            auth_tag: row.access_auth_tag,
            key_version: row.access_key_version,
        };
        let refresh = match (
            row.refresh_ciphertext,
            row.refresh_iv,
            row.refresh_auth_tag,
            row.refresh_key_version,
        ) {
            (Some(ciphertext), Some(iv), Some(auth_tag), Some(key_version)) => Some(StoredSecret {
                ciphertext,
                iv,
                auth_tag,
                key_version,
            }),
            _ => None,
        };
        (access, refresh)
    }))
}

/// The sealed access token a session starts with, and what it was issued
/// for (ADR-171).
pub struct ConnectorAccessToken {
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub sealed: StoredSecret,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct AccessRow {
    issuer: String,
    client_id: String,
    resource: String,
    access_ciphertext: Vec<u8>,
    access_iv: Vec<u8>,
    access_auth_tag: Vec<u8>,
    access_key_version: i32,
    access_expires_at: Option<DateTime<Utc>>,
}

/// The connector's access token, if it is authorized.
pub async fn connector_access_token(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<Option<ConnectorAccessToken>> {
    let row: Option<AccessRow> = sqlx::query_as(
        "select issuer, client_id, resource, access_ciphertext, access_iv, access_auth_tag, \
         access_key_version, access_expires_at from connector_authorizations \
         where connector_id = $1",
    )
    .bind(connector_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(|row| ConnectorAccessToken {
        issuer: row.issuer,
        client_id: row.client_id,
        resource: row.resource,
        sealed: StoredSecret {
            ciphertext: row.access_ciphertext,
            iv: row.access_iv,
            auth_tag: row.access_auth_tag,
            key_version: row.access_key_version,
        },
        expires_at: row.access_expires_at,
    }))
}

/// Removes the connector's authorization; returns whether it had one.
pub async fn delete_connector_authorization(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from connector_authorizations where connector_id = $1")
        .bind(connector_id)
        .execute(&mut *conn)
        .await?;
    let removed = deleted.rows_affected() > 0;
    if removed {
        credential_changed(conn, connector_id).await?;
    }
    Ok(removed)
}

/// A new version of the connector (`updated_at`), untested unless disabled:
/// its session is started again with the token read anew (ADR-171), as
/// after a change of its own token. Dated like every other change of the
/// connector (`update_connector_instance`): when the transaction began.
async fn credential_changed(conn: &mut PgConnection, connector_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "update connector_instances set updated_at = now(), \
         status = case when enabled then 'untested' else status end, \
         last_error = case when enabled then null else last_error end \
         where id = $1",
    )
    .bind(connector_id)
    .execute(conn)
    .await?;
    Ok(())
}
