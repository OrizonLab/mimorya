//! Who each person is on a gateway's service (ADR-125): their Slack user,
//! linked by them from their own session, with the address the service gave
//! as theirs then; or their Telegram user, with the name Telegram showed
//! (ADR-133: it proves no address). One per person and gateway; a user of
//! the service is one person's, so a new link of theirs replaces the former
//! one, whoever's it was.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GatewayIdentityRow {
    pub id: Uuid,
    pub gateway_id: Uuid,
    pub user_id: Uuid,
    pub external_id: String,
    /// The address Slack gave as theirs.
    pub address: Option<String>,
    /// The name Telegram showed.
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

const SELECT: &str = "select id, gateway_id, user_id, external_id, address, display_name, \
     created_at from gateway_identities";

pub struct NewGatewayIdentity<'a> {
    pub tenant_id: Uuid,
    pub gateway_id: Uuid,
    pub user_id: Uuid,
    pub external_id: &'a str,
    /// The address the service gave as theirs, if it proves one.
    pub address: Option<&'a str>,
    /// The name it showed, if it proves no address.
    pub display_name: Option<&'a str>,
}

/// What a new link replaced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replaced {
    /// The person's former identity on this gateway, if another one.
    pub own: Option<String>,
    /// Another person this identity was linked to, if any.
    pub other_person: Option<Uuid>,
}

/// Links the person to `external_id` on the gateway: their former identity
/// there, and this identity's link to anyone else, are replaced.
pub async fn put_gateway_identity(
    conn: &mut PgConnection,
    identity: NewGatewayIdentity<'_>,
) -> sqlx::Result<Replaced> {
    let other_person: Option<Uuid> = sqlx::query_scalar(
        "delete from gateway_identities where gateway_id = $1 and external_id = $2 \
         and user_id <> $3 returning user_id",
    )
    .bind(identity.gateway_id)
    .bind(identity.external_id)
    .bind(identity.user_id)
    .fetch_optional(&mut *conn)
    .await?;
    let own: Option<String> = sqlx::query_scalar(
        "select external_id from gateway_identities where gateway_id = $1 and user_id = $2",
    )
    .bind(identity.gateway_id)
    .bind(identity.user_id)
    .fetch_optional(&mut *conn)
    .await?;
    sqlx::query(
        "insert into gateway_identities \
         (tenant_id, gateway_id, kind, user_id, external_id, address, display_name) \
         values ($1, $2, (select kind from gateways where id = $2), $3, $4, $5, $6) \
         on conflict (gateway_id, user_id) do update set external_id = excluded.external_id, \
         address = excluded.address, display_name = excluded.display_name, created_at = now()",
    )
    .bind(identity.tenant_id)
    .bind(identity.gateway_id)
    .bind(identity.user_id)
    .bind(identity.external_id)
    .bind(identity.address)
    .bind(identity.display_name)
    .execute(conn)
    .await?;
    Ok(Replaced {
        own: own.filter(|former| former != identity.external_id),
        other_person,
    })
}

/// The identity `external_id` on the gateway, if a person linked it.
pub async fn find_gateway_identity(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    external_id: &str,
) -> sqlx::Result<Option<GatewayIdentityRow>> {
    sqlx::query_as(&format!(
        "{SELECT} where gateway_id = $1 and external_id = $2"
    ))
    .bind(gateway_id)
    .bind(external_id)
    .fetch_optional(conn)
    .await
}

/// The person `user_id`'s identity on the gateway, if they linked one.
pub async fn gateway_identity_of(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<GatewayIdentityRow>> {
    sqlx::query_as(&format!("{SELECT} where gateway_id = $1 and user_id = $2"))
        .bind(gateway_id)
        .bind(user_id)
        .fetch_optional(conn)
        .await
}

/// The person's identities, on every gateway of their company.
pub async fn list_gateway_identities_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<GatewayIdentityRow>> {
    sqlx::query_as(&format!("{SELECT} where user_id = $1 order by created_at"))
        .bind(user_id)
        .fetch_all(conn)
        .await
}

/// How many people linked their identity on the gateway.
pub async fn count_gateway_identities(
    conn: &mut PgConnection,
    gateway_id: Uuid,
) -> sqlx::Result<i64> {
    sqlx::query_scalar("select count(*) from gateway_identities where gateway_id = $1")
        .bind(gateway_id)
        .fetch_one(conn)
        .await
}

/// Removes the person's identity on the gateway; the one removed, if any.
pub async fn delete_gateway_identity(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar(
        "delete from gateway_identities where gateway_id = $1 and user_id = $2 \
         returning external_id",
    )
    .bind(gateway_id)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Removes the person's identities on every gateway (their account is
/// deactivated); how many.
pub async fn delete_gateway_identities_of(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<u64> {
    let deleted = sqlx::query("delete from gateway_identities where user_id = $1")
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected())
}

/// The account a Slack user is linked to on a gateway, as it stands (a
/// deactivated account has no identity left).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LinkedAccount {
    pub user_id: Uuid,
    pub email: String,
    pub must_change_password: bool,
}

/// The account `external_id` is linked to on the gateway, if any.
pub async fn linked_account(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    external_id: &str,
) -> sqlx::Result<Option<LinkedAccount>> {
    sqlx::query_as(
        "select u.id as user_id, u.email::text as email, u.must_change_password \
         from gateway_identities i join users u on u.id = i.user_id \
         where i.gateway_id = $1 and i.external_id = $2",
    )
    .bind(gateway_id)
    .bind(external_id)
    .fetch_optional(conn)
    .await
}

/// Removes the link of `external_id` on the gateway, if it is still to the
/// person `user_id`; whether it was.
pub async fn unlink_gateway_identity(
    conn: &mut PgConnection,
    gateway_id: Uuid,
    external_id: &str,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let deleted = sqlx::query(
        "delete from gateway_identities where gateway_id = $1 and external_id = $2 \
         and user_id = $3",
    )
    .bind(gateway_id)
    .bind(external_id)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(deleted.rows_affected() == 1)
}
