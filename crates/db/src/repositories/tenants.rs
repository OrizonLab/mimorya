//! Companies created, suspended and reactivated by the platform's operator
//! (ADR-033, ADR-191). Each change runs in the company's own transaction
//! (`Database::begin` with its id): the row-level security of `tenants` lets
//! a transaction write and read its tenant only. The operator names a
//! company by its slug, found before entering it.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

pub struct NewTenant<'a> {
    /// Chosen by the caller, who opened the transaction with it.
    pub id: Uuid,
    /// Unique across the platform.
    pub slug: &'a str,
    pub name: &'a str,
}

/// A real company: active, default settings, no demonstration flag.
pub async fn create_tenant(conn: &mut PgConnection, input: NewTenant<'_>) -> sqlx::Result<()> {
    sqlx::query("insert into tenants (id, slug, name) values ($1, $2, $3)")
        .bind(input.id)
        .bind(input.slug)
        .bind(input.name)
        .execute(conn)
        .await?;
    Ok(())
}

/// `active` or `suspended`, as stored now.
pub async fn tenant_status(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("select status from tenants where id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// The company's status, its row locked until the end of the transaction:
/// a change of its status, or its deletion, waits for it (ADR-193). Not
/// the rows that name it: a key share, as each of them takes, goes on.
pub async fn lock_tenant_status(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("select status from tenants where id = $1 for no key update")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// The company's slug, as its operator knows it.
pub async fn tenant_slug(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("select slug from tenants where id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// The company whose slug is `slug`, found before entering it, as the
/// operator names it (ADR-191): its identifier alone.
pub async fn find_tenant_by_slug(pool: &PgPool, slug: &str) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("select system_find_tenant($1)")
        .bind(slug)
        .fetch_one(pool)
        .await
}

/// Moves the company from status `from` to `to`; false if it was not in
/// `from` (changed meanwhile).
pub async fn change_tenant_status(
    conn: &mut PgConnection,
    id: Uuid,
    from: &str,
    to: &str,
) -> sqlx::Result<bool> {
    let changed = sqlx::query(
        "update tenants set status = $3, updated_at = now() where id = $1 and status = $2",
    )
    .bind(id)
    .bind(from)
    .bind(to)
    .execute(conn)
    .await?;
    Ok(changed.rows_affected() == 1)
}
