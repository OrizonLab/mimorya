//! Conversation gateways a company declares (ADR-125): one per service and
//! company, with what the service said of the application when it was
//! declared or last checked, its token's permissions among it (ADR-132); a
//! request from the service finds its company by the gateway it names; the
//! events taken are kept by identifier a day, so that a retry is taken
//! once.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GatewayRow {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub enabled: bool,
    pub workspace_id: String,
    pub workspace_name: String,
    pub bot_user_id: String,
    /// The permissions the service said the application's token has, if
    /// it said them.
    pub granted_scopes: Option<Vec<String>>,
    /// When the service was last asked.
    pub scopes_checked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT: &str = "select id, kind, name, enabled, workspace_id, workspace_name, bot_user_id, \
    granted_scopes, scopes_checked_at, created_at, updated_at from gateways";

pub struct NewGateway<'a> {
    pub tenant_id: Uuid,
    pub kind: &'a str,
    pub name: &'a str,
    pub enabled: bool,
    pub workspace_id: &'a str,
    pub workspace_name: &'a str,
    pub bot_user_id: &'a str,
    /// The permissions the service said its token has, if it said them.
    pub granted_scopes: Option<&'a [String]>,
}

/// What a change sets: `None` leaves a field as it stands.
#[derive(Default)]
pub struct GatewayPatch<'a> {
    pub name: Option<&'a str>,
    pub enabled: Option<bool>,
    /// What the service says of the application, read again with its new
    /// token: its workspace's name and its bot user.
    pub workspace_name: Option<&'a str>,
    pub bot_user_id: Option<&'a str>,
    /// The service asked again: the permissions it said the token has, if
    /// it said them.
    pub granted_scopes: Option<Option<&'a [String]>>,
}

/// The company's gateways, by service.
pub async fn list_gateways(conn: &mut PgConnection) -> sqlx::Result<Vec<GatewayRow>> {
    sqlx::query_as(&format!("{SELECT} order by kind, created_at"))
        .fetch_all(conn)
        .await
}

/// The company's gateway `id`.
pub async fn get_gateway(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<GatewayRow>> {
    sqlx::query_as(&format!("{SELECT} where id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// The company's gateway `id`, locked against another change.
pub async fn lock_gateway(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<GatewayRow>> {
    sqlx::query_as(&format!("{SELECT} where id = $1 for update"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Declares a gateway, its permissions read now (only a Slack application
/// has any);
/// fails on a unique violation when the company has one of this service
/// already.
pub async fn create_gateway(
    conn: &mut PgConnection,
    gateway: NewGateway<'_>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into gateways (tenant_id, kind, name, enabled, workspace_id, workspace_name, \
         bot_user_id, granted_scopes, scopes_checked_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, \
         case when $2 = 'slack' then now() end) returning id",
    )
    .bind(gateway.tenant_id)
    .bind(gateway.kind)
    .bind(gateway.name)
    .bind(gateway.enabled)
    .bind(gateway.workspace_id)
    .bind(gateway.workspace_name)
    .bind(gateway.bot_user_id)
    .bind(gateway.granted_scopes)
    .fetch_one(conn)
    .await
}

/// Applies `patch` to the gateway `id`.
pub async fn update_gateway(
    conn: &mut PgConnection,
    id: Uuid,
    patch: GatewayPatch<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update gateways set name = coalesce($2, name), enabled = coalesce($3, enabled), \
         workspace_name = coalesce($4, workspace_name), \
         bot_user_id = coalesce($5, bot_user_id), \
         granted_scopes = case when $6 then $7 else granted_scopes end, \
         scopes_checked_at = case when $6 then now() else scopes_checked_at end, \
         updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(patch.name)
    .bind(patch.enabled)
    .bind(patch.workspace_name)
    .bind(patch.bot_user_id)
    .bind(patch.granted_scopes.is_some())
    .bind(patch.granted_scopes.flatten())
    .execute(conn)
    .await?;
    Ok(())
}

/// Keeps what the service said of the gateway `id`'s application when it
/// was asked again (ADR-132): its workspace's name, its bot user and its
/// token's permissions. Not a change of the gateway: `updated_at` stays.
pub async fn record_gateway_check(
    conn: &mut PgConnection,
    id: Uuid,
    (workspace_name, bot_user_id): (&str, &str),
    granted_scopes: Option<&[String]>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update gateways set workspace_name = $2, bot_user_id = $3, granted_scopes = $4, \
         scopes_checked_at = now() where id = $1",
    )
    .bind(id)
    .bind(workspace_name)
    .bind(bot_user_id)
    .bind(granted_scopes)
    .execute(conn)
    .await?;
    Ok(())
}

/// Deletes the gateway `id`, with its people's identities and its events;
/// whether there was one.
pub async fn delete_gateway(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from gateways where id = $1")
        .bind(id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// The company of the gateway `id` of `kind`, before any session (a
/// request from its service names it).
pub async fn gateway_tenant(pool: &PgPool, id: Uuid, kind: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("select auth_gateway_tenant($1, $2)")
        .bind(id)
        .bind(kind)
        .fetch_one(pool)
        .await
}

/// Keeps that the gateway took the event `event_id`; false when it was
/// taken already (the service sent it again).
pub async fn take_gateway_event(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    gateway_id: Uuid,
    event_id: &str,
) -> sqlx::Result<bool> {
    let taken = sqlx::query(
        "insert into gateway_events (tenant_id, gateway_id, event_id) values ($1, $2, $3) \
         on conflict (gateway_id, event_id) do nothing",
    )
    .bind(tenant_id)
    .bind(gateway_id)
    .bind(event_id)
    .execute(conn)
    .await?;
    Ok(taken.rows_affected() > 0)
}

/// Forgets the events every company's gateways took before `before`; how
/// many.
pub async fn purge_gateway_events(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<i64> {
    sqlx::query_scalar("select system_purge_gateway_events($1)")
        .bind(before)
        .fetch_one(pool)
        .await
}
