//! The OAuth authorizations of an MCP server each person gives for themself
//! (ADR-174): the tokens its authorization server issued to that person,
//! sealed by the API before they reach the database, with what they were
//! issued for and when. One per connector and person; a new one replaces
//! the person's previous, not revoked (ADR-172). Unlike the company's
//! authorization (ADR-171), keeping or removing one makes no new version of
//! the connector: a person's session is versioned by their own
//! authorization. Every read and every removal names the person, except
//! the count of a connector's (how many, never who) and those that take all
//! of a connector's when the administration changes the declaration they
//! were given under, or all of a person's when their account is disabled.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use super::connector_authorizations::{
    AccessRow, ConnectorAccessToken, SealedRow, TakenAuthorization,
};
use super::secrets::StoredSecret;

/// A person's authorization as listed: never its sealed tokens.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersonAuthorizationRow {
    pub connector_id: Uuid,
    pub user_id: Uuid,
    pub issuer: String,
    pub client_id: String,
    pub resource: String,
    pub scopes: String,
    pub access_expires_at: Option<DateTime<Utc>>,
    /// Whether a refresh token is kept.
    pub renewable: bool,
    pub authorized_at: DateTime<Utc>,
    pub refreshed_at: DateTime<Utc>,
    /// Why its last renewal failed, the authorization kept.
    pub renewal_failure: Option<String>,
}

pub struct NewPersonAuthorization<'a> {
    pub tenant_id: Uuid,
    pub connector_id: Uuid,
    pub user_id: Uuid,
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub resource: &'a str,
    pub scopes: &'a str,
    pub access_token: &'a StoredSecret,
    pub access_expires_at: Option<DateTime<Utc>>,
    pub refresh_token: Option<&'a StoredSecret>,
}

const LISTED: &str = "connector_id, user_id, issuer, client_id, resource, scopes, \
     access_expires_at, refresh_ciphertext is not null as renewable, authorized_at, \
     refreshed_at, renewal_failure";

const TAKEN: &str = "connector_id, user_id, issuer, client_id, resource, access_ciphertext, \
     access_iv, access_auth_tag, access_key_version, refresh_ciphertext, refresh_iv, \
     refresh_auth_tag, refresh_key_version";

/// Keeps the person's authorization of the connector, replacing their
/// previous one; returns whether one was replaced.
pub async fn put_person_authorization(
    conn: &mut PgConnection,
    new: NewPersonAuthorization<'_>,
) -> sqlx::Result<bool> {
    let replaced = sqlx::query(
        "delete from connector_person_authorizations where connector_id = $1 and user_id = $2",
    )
    .bind(new.connector_id)
    .bind(new.user_id)
    .execute(&mut *conn)
    .await?
    .rows_affected()
        > 0;
    let refresh = new.refresh_token;
    sqlx::query(
        "insert into connector_person_authorizations (tenant_id, connector_id, user_id, \
         issuer, client_id, resource, scopes, access_ciphertext, access_iv, access_auth_tag, \
         access_key_version, access_expires_at, refresh_ciphertext, refresh_iv, \
         refresh_auth_tag, refresh_key_version) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(new.tenant_id)
    .bind(new.connector_id)
    .bind(new.user_id)
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
    .execute(conn)
    .await?;
    Ok(replaced)
}

/// The person's authorization of the connector, if they gave one.
pub async fn get_person_authorization(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<PersonAuthorizationRow>> {
    sqlx::query_as(&format!(
        "select {LISTED} from connector_person_authorizations \
         where connector_id = $1 and user_id = $2"
    ))
    .bind(connector_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// The authorizations the person gave, the oldest first.
pub async fn person_authorizations_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<PersonAuthorizationRow>> {
    sqlx::query_as(&format!(
        "select {LISTED} from connector_person_authorizations where user_id = $1 \
         order by authorized_at, connector_id"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// How many people authorized the connector: never who.
pub async fn count_person_authorizations(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select count(*) from connector_person_authorizations where connector_id = $1",
    )
    .bind(connector_id)
    .fetch_one(conn)
    .await
}

/// The person's access token for the connector, if they authorized it.
pub async fn person_access_token(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<ConnectorAccessToken>> {
    let row: Option<AccessRow> = sqlx::query_as(
        "select issuer, client_id, resource, access_ciphertext, access_iv, access_auth_tag, \
         access_key_version, access_expires_at from connector_person_authorizations \
         where connector_id = $1 and user_id = $2",
    )
    .bind(connector_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(ConnectorAccessToken::from))
}

/// A person's authorization taken away, with whose it was and what revoking
/// its tokens needs (ADR-172).
pub struct TakenPersonAuthorization {
    pub connector_id: Uuid,
    pub user_id: Uuid,
    pub taken: TakenAuthorization,
}

#[derive(sqlx::FromRow)]
struct TakenRow {
    connector_id: Uuid,
    user_id: Uuid,
    issuer: String,
    client_id: String,
    resource: String,
    #[sqlx(flatten)]
    sealed: SealedRow,
}

impl From<TakenRow> for TakenPersonAuthorization {
    fn from(row: TakenRow) -> Self {
        let (access, refresh) = row.sealed.tokens();
        Self {
            connector_id: row.connector_id,
            user_id: row.user_id,
            taken: TakenAuthorization {
                issuer: row.issuer,
                client_id: row.client_id,
                resource: row.resource,
                access,
                refresh,
            },
        }
    }
}

/// Removes the person's authorization of the connector, if they gave one,
/// and returns what revoking it needs.
pub async fn take_person_authorization(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<TakenPersonAuthorization>> {
    let row: Option<TakenRow> = sqlx::query_as(&format!(
        "delete from connector_person_authorizations where connector_id = $1 and user_id = $2 \
         returning {TAKEN}"
    ))
    .bind(connector_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.map(TakenPersonAuthorization::from))
}

/// Removes every person's authorization of the connector, its declaration
/// changing or the connector going, and returns what revoking them needs.
pub async fn take_person_authorizations_of_connector(
    conn: &mut PgConnection,
    connector_id: Uuid,
) -> sqlx::Result<Vec<TakenPersonAuthorization>> {
    let rows: Vec<TakenRow> = sqlx::query_as(&format!(
        "delete from connector_person_authorizations where connector_id = $1 returning {TAKEN}"
    ))
    .bind(connector_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(TakenPersonAuthorization::from)
        .collect())
}

/// Removes every authorization the person gave, their account being
/// disabled, and returns what revoking them needs.
pub async fn take_person_authorizations_of_user(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<TakenPersonAuthorization>> {
    let rows: Vec<TakenRow> = sqlx::query_as(&format!(
        "delete from connector_person_authorizations where user_id = $1 returning {TAKEN}"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(TakenPersonAuthorization::from)
        .collect())
}
