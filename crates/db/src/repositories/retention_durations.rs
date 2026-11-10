//! A company's own retention durations (ADR-195), one per kind, read in
//! its transaction with what the purge applies now and when a shortening
//! will apply. The application writes a duration and who chose it; the
//! database keeps what applied before it and when it was chosen, and
//! applies the longer of the two for seven days (`retention_in_effect`).

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

/// A duration the company set.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct StoredRetention {
    pub kind: String,
    /// The one it chose.
    pub duration: i32,
    /// What the purge applies now: the longer of the chosen one and what
    /// applied before it, for seven days after it was chosen.
    pub in_effect: i32,
    pub changed_at: DateTime<Utc>,
    /// When a shortening chosen at `changed_at` applies.
    pub shortening_applies_at: DateTime<Utc>,
}

/// The durations the company set, by kind.
pub async fn retention_durations(conn: &mut PgConnection) -> sqlx::Result<Vec<StoredRetention>> {
    sqlx::query_as(
        "select kind, duration, \
         retention_in_effect(duration, before_duration, changed_at) as in_effect, changed_at, \
         retention_shortening_applies_at(changed_at) as shortening_applies_at \
         from tenant_retention_durations order by kind",
    )
    .fetch_all(conn)
    .await
}

/// Sets the company's duration of `kind` to `duration`, chosen by
/// `updated_by_user_id`: the database keeps what applied until now as what
/// applied before it.
pub async fn set_retention_duration(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    kind: &str,
    duration: i32,
    updated_by_user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into tenant_retention_durations (tenant_id, kind, duration, updated_by_user_id) \
         values ($1, $2, $3, $4) \
         on conflict (tenant_id, kind) do update set duration = excluded.duration, \
         updated_by_user_id = excluded.updated_by_user_id",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(duration)
    .bind(updated_by_user_id)
    .execute(conn)
    .await?;
    Ok(())
}
