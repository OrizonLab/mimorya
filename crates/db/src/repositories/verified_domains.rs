//! The e-mail domains a company proved it holds (ADR-069), by a TXT record
//! of its own in their DNS. Rows are the company's (RLS).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct VerifiedDomain {
    pub domain: String,
    pub verified_at: DateTime<Utc>,
}

pub async fn list_verified_domains(conn: &mut PgConnection) -> sqlx::Result<Vec<VerifiedDomain>> {
    sqlx::query_as(
        "select domain::text as domain, verified_at from verified_domains \
         order by domain::text collate \"C\"",
    )
    .fetch_all(conn)
    .await
}

pub async fn is_domain_verified(conn: &mut PgConnection, domain: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("select exists (select 1 from verified_domains where domain = $1)")
        .bind(domain)
        .fetch_one(conn)
        .await
}

/// Whether one of the company's providers covers the domain.
pub async fn is_domain_declared(conn: &mut PgConnection, domain: &str) -> sqlx::Result<bool> {
    sqlx::query_scalar("select exists (select 1 from identity_provider_domains where domain = $1)")
        .bind(domain)
        .fetch_one(conn)
        .await
}

/// Proved now, by `user_id`; proved again, the date moves.
pub async fn record_verified_domain(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    domain: &str,
    user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into verified_domains (tenant_id, domain, verified_by_user_id) values ($1, $2, $3) \
         on conflict (tenant_id, domain) do update \
         set verified_at = now(), verified_by_user_id = excluded.verified_by_user_id",
    )
    .bind(tenant_id)
    .bind(domain)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(())
}
