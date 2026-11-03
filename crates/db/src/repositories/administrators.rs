//! The accounts that may administer something of their company: active,
//! holding, unexpired, a role that grants the permission on the company or
//! a unit. Whether each holds it on the whole company is then checked with
//! the company's rules (`holds_company_wide`): a unit's scope, or a deny
//! policy, may take it away. Who is told of what awaits an administrator's
//! decision (ADR-102, ADR-170).

use sqlx::PgConnection;
use uuid::Uuid;

/// An active account of the company, to tell if it may decide.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Recipient {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
}

/// The company's active accounts holding, unexpired, a role that grants
/// `permission` on the company or a unit, by name.
pub async fn possible_administrators(
    conn: &mut PgConnection,
    permission: &str,
) -> sqlx::Result<Vec<Recipient>> {
    sqlx::query_as(
        "select distinct u.id as user_id, u.email::text as email, u.display_name \
         from users u \
         join role_assignments a on a.user_id = u.id \
         join roles r on r.id = a.role_id \
         where u.status = 'active' and a.scope_type <> 'self' \
         and $1 = any(r.permissions) \
         and (a.expires_at is null or a.expires_at > now()) \
         order by u.display_name",
    )
    .bind(permission)
    .fetch_all(conn)
    .await
}
