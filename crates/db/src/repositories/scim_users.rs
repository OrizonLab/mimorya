//! The company's accounts as its identity provider sees them through SCIM
//! (ADR-183): every account of the company but those it removed, its
//! address, name, job title, status, the provider's own identifier of it
//! when it gave one, and when it was made and last changed; read page by
//! page in the order they were made, or one at a time by its identifier,
//! its address or the provider's identifier. An account the provider
//! creates: active or disabled, without a password, of the default
//! clearance. The provider's identifier given, changed or taken away; an
//! account removed, and there again once an administrator enables it.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// An account as SCIM shows it.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ScimUserRow {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub job_title: Option<String>,
    /// `active`, `invited` or `disabled`.
    pub status: String,
    pub external_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const SELECT: &str = "select u.id, u.email::text as email, u.display_name, u.job_title, \
    u.status, s.external_id, u.created_at, u.updated_at \
    from users u left join scim_accounts s on s.user_id = u.id";

/// Of `SELECT`, the accounts the provider has not removed.
const THERE: &str = "s.removed_at is null";

/// How many accounts the provider sees.
pub async fn count_scim_users(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar(&format!(
        "select count(*) from users u left join scim_accounts s on s.user_id = u.id \
         where {THERE}"
    ))
    .fetch_one(conn)
    .await
}

/// `limit` of the company's accounts after the first `offset`, in the order
/// they were made.
pub async fn scim_users_page(
    conn: &mut PgConnection,
    offset: i64,
    limit: i64,
) -> sqlx::Result<Vec<ScimUserRow>> {
    sqlx::query_as(&format!(
        "{SELECT} where {THERE} order by u.created_at, u.id offset $1 limit $2"
    ))
    .bind(offset)
    .bind(limit)
    .fetch_all(conn)
    .await
}

/// The company's account `id`.
pub async fn scim_user(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<ScimUserRow>> {
    sqlx::query_as(&format!("{SELECT} where u.id = $1 and {THERE}"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// The company's account at `email`, compared without case.
pub async fn scim_user_by_email(
    conn: &mut PgConnection,
    email: &str,
) -> sqlx::Result<Option<ScimUserRow>> {
    sqlx::query_as(&format!("{SELECT} where u.email = $1::citext and {THERE}"))
        .bind(email)
        .fetch_optional(conn)
        .await
}

/// The company's account the provider identifies as `external_id`,
/// compared exactly.
pub async fn scim_user_by_external_id(
    conn: &mut PgConnection,
    external_id: &str,
) -> sqlx::Result<Option<ScimUserRow>> {
    sqlx::query_as(&format!("{SELECT} where s.external_id = $1 and {THERE}"))
        .bind(external_id)
        .fetch_optional(conn)
        .await
}

/// An account the provider creates, at `email` (lowercase): active or
/// disabled, without a password, of the default clearance, in no unit.
/// `None` when the address is taken, in this company or in another:
/// addresses are unique across companies (ADR-011).
pub async fn create_provisioned_user(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    email: &str,
    display_name: &str,
    job_title: Option<&str>,
    active: bool,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar(
        "insert into users (tenant_id, email, display_name, job_title, status, activated) \
         values ($1, $2, $3, $4, case when $5 then 'active' else 'disabled' end, true) \
         on conflict (email) do nothing returning id",
    )
    .bind(tenant_id)
    .bind(email)
    .bind(display_name)
    .bind(job_title)
    .bind(active)
    .fetch_optional(conn)
    .await
}

/// Ties the provider's identifier `external_id` to the account `user_id`;
/// `false` when the company already ties it to another account.
pub async fn tie_external_id(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    external_id: &str,
) -> sqlx::Result<bool> {
    let tied = sqlx::query(
        "insert into scim_accounts (tenant_id, user_id, external_id) values ($1, $2, $3) \
         on conflict (tenant_id, external_id) do nothing",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(external_id)
    .execute(conn)
    .await?;
    Ok(tied.rows_affected() == 1)
}

/// Whether the company ties the provider's identifier `external_id` to an
/// account, other than `except` when it names one, removed or not: a removed
/// account keeps its identifier until an administrator enables it again.
pub async fn external_id_taken(
    conn: &mut PgConnection,
    except: Option<Uuid>,
    external_id: &str,
) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "select exists (select 1 from scim_accounts \
         where external_id = $2 and user_id is distinct from $1)",
    )
    .bind(except)
    .bind(external_id)
    .fetch_one(conn)
    .await
}

/// Gives the account `user_id` the provider's identifier `external_id`, or
/// takes it away (`None`).
pub async fn set_external_id(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    external_id: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into scim_accounts (tenant_id, user_id, external_id) values ($1, $2, $3) \
         on conflict (tenant_id, user_id) do update \
         set external_id = excluded.external_id, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(external_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks the account `user_id` removed by the provider: no longer there for
/// SCIM.
pub async fn mark_scim_removed(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into scim_accounts (tenant_id, user_id, removed_at) values ($1, $2, now()) \
         on conflict (tenant_id, user_id) do update set removed_at = now(), updated_at = now()",
    )
    .bind(tenant_id)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(())
}

/// The account `user_id` there again for SCIM, if the provider removed it.
pub async fn clear_scim_removal(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "update scim_accounts set removed_at = null, updated_at = now() \
         where user_id = $1 and removed_at is not null",
    )
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(())
}
