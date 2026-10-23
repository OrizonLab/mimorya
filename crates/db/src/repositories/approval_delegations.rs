//! Who stands in for an absent person on the requests a manager decides
//! (ADR-071): one delegation per person, to a colleague, for a period,
//! declared by the person or by an account administrator on their behalf
//! (ADR-078). It acts only while it runs and the absent person's account is
//! active, and grants no right: the delegate decides only where they hold
//! `approval:decide`.

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
    /// The administrator who declared it on the absent person's behalf.
    pub declared_by_user_id: Option<Uuid>,
    pub declared_by_name: Option<String>,
}

const NAMED: &str = "select d.user_id, u.display_name as user_name, d.delegate_user_id, \
     g.display_name as delegate_name, d.starts_at, d.ends_at, d.declared_by_user_id, \
     b.display_name as declared_by_name \
     from approval_delegations d \
     join users u on u.id = d.user_id \
     join users g on g.id = d.delegate_user_id \
     left join users b on b.id = d.declared_by_user_id";

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

/// A delegation to declare: by the absent person when `declared_by` is
/// `None`, else by that administrator on their behalf.
pub struct NewDelegation {
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub delegate_user_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub declared_by: Option<Uuid>,
}

/// Declares (or replaces) the delegation of `user_id`.
pub async fn set_delegation(conn: &mut PgConnection, new: NewDelegation) -> sqlx::Result<()> {
    sqlx::query(
        "insert into approval_delegations \
         (tenant_id, user_id, delegate_user_id, starts_at, ends_at, declared_by_user_id) \
         values ($1, $2, $3, $4, $5, $6) \
         on conflict (tenant_id, user_id) do update set \
         delegate_user_id = excluded.delegate_user_id, starts_at = excluded.starts_at, \
         ends_at = excluded.ends_at, declared_by_user_id = excluded.declared_by_user_id, \
         created_at = now()",
    )
    .bind(new.tenant_id)
    .bind(new.user_id)
    .bind(new.delegate_user_id)
    .bind(new.starts_at)
    .bind(new.ends_at)
    .bind(new.declared_by)
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

/// A colleague one may name to stand in for oneself.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Colleague {
    pub user_id: Uuid,
    pub display_name: String,
}

/// The colleagues of `user_id` who may stand in for them, as far as their
/// roles say at `now`: active accounts holding `approval:decide` on a unit
/// or the whole company. Whether they may decide a given request is
/// checked then, over whom it is for.
pub async fn possible_stand_ins(
    conn: &mut PgConnection,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<Colleague>> {
    sqlx::query_as(
        "select distinct u.id as user_id, u.display_name from users u \
         join role_assignments a on a.user_id = u.id \
         join roles r on r.id = a.role_id \
         where u.id <> $1 and u.status = 'active' and a.scope_type <> 'self' \
         and 'approval:decide' = any(r.permissions) \
         and (a.expires_at is null or a.expires_at > $2) \
         order by u.display_name",
    )
    .bind(user_id)
    .bind(now)
    .fetch_all(conn)
    .await
}

/// Deletes every company's ended delegations; how many.
pub async fn purge_ended_approval_delegations(pool: &PgPool) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_ended_approval_delegations()")
        .fetch_one(pool)
        .await
}
