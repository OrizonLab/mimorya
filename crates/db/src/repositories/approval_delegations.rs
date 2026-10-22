//! Who stands in for an absent person on the requests a manager decides
//! (ADR-071): one delegation per person, to a colleague, for a period. It
//! acts only while it runs and its author's account is active, and grants
//! no right: the delegate decides only where they hold `approval:decide`.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A delegation, with the names of whom it is for and of their delegate.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApprovalDelegation {
    pub user_id: Uuid,
    pub user_name: String,
    pub delegate_user_id: Uuid,
    pub delegate_name: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
}

const NAMED: &str = "select d.user_id, u.display_name as user_name, d.delegate_user_id, \
     g.display_name as delegate_name, d.starts_at, d.ends_at \
     from approval_delegations d \
     join users u on u.id = d.user_id \
     join users g on g.id = d.delegate_user_id";

/// The delegation `user_id` declared, unless it ended by `now`.
pub async fn get_delegation(
    conn: &mut PgConnection,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<ApprovalDelegation>> {
    sqlx::query_as(&format!("{NAMED} where d.user_id = $1 and d.ends_at > $2"))
        .bind(user_id)
        .bind(now)
        .fetch_optional(conn)
        .await
}

/// Declares (or replaces) the delegation of `user_id`.
pub async fn set_delegation(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    delegate_user_id: Uuid,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into approval_delegations \
         (tenant_id, user_id, delegate_user_id, starts_at, ends_at) \
         values ($1, $2, $3, $4, $5) \
         on conflict (tenant_id, user_id) do update set \
         delegate_user_id = excluded.delegate_user_id, starts_at = excluded.starts_at, \
         ends_at = excluded.ends_at, created_at = now()",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(delegate_user_id)
    .bind(starts_at)
    .bind(ends_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Withdraws the delegation of `user_id`; whether one had not ended yet.
pub async fn delete_delegation(
    conn: &mut PgConnection,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let deleted: Option<DateTime<Utc>> =
        sqlx::query_scalar("delete from approval_delegations where user_id = $1 returning ends_at")
            .bind(user_id)
            .fetch_optional(conn)
            .await?;
    Ok(deleted.is_some_and(|ends_at| ends_at > now))
}

/// The delegations to `delegate_user_id` that have not ended by `now`, of
/// active accounts, soonest first.
pub async fn delegations_to(
    conn: &mut PgConnection,
    delegate_user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<ApprovalDelegation>> {
    sqlx::query_as(&format!(
        "{NAMED} where d.delegate_user_id = $1 and d.ends_at > $2 and u.status = 'active' \
         order by d.starts_at, u.display_name"
    ))
    .bind(delegate_user_id)
    .bind(now)
    .fetch_all(conn)
    .await
}

/// Those `delegate_user_id` stands in for at `now`: their delegation runs,
/// and their account is active. One hop only: whom these stand in for is
/// not passed on.
pub async fn standing_in_for(
    conn: &mut PgConnection,
    delegate_user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "select d.user_id from approval_delegations d join users u on u.id = d.user_id \
         where d.delegate_user_id = $1 and d.starts_at <= $2 and d.ends_at > $2 \
         and u.status = 'active'",
    )
    .bind(delegate_user_id)
    .bind(now)
    .fetch_all(conn)
    .await
}

/// A colleague standing in for someone, as their messages name them.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StandIn {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
}

/// Whom `user_id` named to stand in for them at `now`: their delegation
/// runs, and both accounts are active.
pub async fn stand_in_for(
    conn: &mut PgConnection,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Option<StandIn>> {
    sqlx::query_as(
        "select g.id as user_id, g.email::text as email, g.display_name \
         from approval_delegations d \
         join users u on u.id = d.user_id \
         join users g on g.id = d.delegate_user_id \
         where d.user_id = $1 and d.starts_at <= $2 and d.ends_at > $2 \
         and u.status = 'active' and g.status = 'active'",
    )
    .bind(user_id)
    .bind(now)
    .fetch_optional(conn)
    .await
}

/// Deletes every company's ended delegations; how many.
pub async fn purge_ended_approval_delegations(pool: &PgPool) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_ended_approval_delegations()")
        .fetch_one(pool)
        .await
}
