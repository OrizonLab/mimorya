//! Companies created by the platform's operator (ADR-033). The insert runs
//! in the new company's own transaction (`Database::begin` with its id): the
//! row-level security of `tenants` lets a transaction write its tenant only.

use sqlx::PgConnection;
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
