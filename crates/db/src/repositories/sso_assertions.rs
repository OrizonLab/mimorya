//! The assertions a SAML provider's returns carried (ADR-068), kept until
//! they expire: each is accepted once, whichever server receives it.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// Whether the assertion was new; the provider's expired ones go first.
pub async fn record_assertion(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    provider_id: Uuid,
    assertion_id: &str,
    expires_at: DateTime<Utc>,
) -> sqlx::Result<bool> {
    sqlx::query("delete from sso_used_assertions where provider_id = $1 and expires_at < now()")
        .bind(provider_id)
        .execute(&mut *conn)
        .await?;
    let recorded = sqlx::query(
        "insert into sso_used_assertions (tenant_id, provider_id, assertion_id, expires_at) \
         values ($1, $2, $3, $4) on conflict do nothing",
    )
    .bind(tenant_id)
    .bind(provider_id)
    .bind(assertion_id)
    .bind(expires_at)
    .execute(conn)
    .await?;
    Ok(recorded.rows_affected() == 1)
}
