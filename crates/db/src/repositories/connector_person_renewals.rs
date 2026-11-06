//! A person's own OAuth access token of an MCP server renewed (ADR-174), as
//! the company's is (`connector_renewals`, ADR-171): claimed for a while by
//! one statement, so that another process skips it and a token endpoint
//! that did not answer is not asked again before the claim ends; what the
//! renewal brings kept, or the authorization removed, only if it is still
//! the one that was claimed (same row, not renewed since). Unlike the
//! company's, it makes no new version of the connector: the person's
//! session follows their own authorization's `refreshed_at`. Every claim
//! names the person.

use sqlx::PgConnection;
use uuid::Uuid;

use super::connector_renewals::{
    CLAIM, CLAIMED, Claimed, ClaimedRenewal, RenewalFor, Renewed, claim_query, keep_in,
    remove_claimed_from, say_failure_in,
};

/// Where the people's own authorizations are.
const PERSON: &str = "connector_person_authorizations";

/// The enabled connectors whose access token the person's authorization
/// lets expire within `within`, that can be renewed and whose renewal is
/// not claimed.
pub async fn person_authorizations_due(
    conn: &mut PgConnection,
    user_id: Uuid,
    within: std::time::Duration,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "select a.connector_id from connector_person_authorizations a \
         join connector_instances i on i.id = a.connector_id and i.enabled \
         where a.user_id = $1 and a.refresh_ciphertext is not null \
         and a.access_expires_at < now() + make_interval(secs => $2) \
         and (a.renewing_until is null or a.renewing_until <= now()) order by a.connector_id",
    )
    .bind(user_id)
    .bind(within.as_secs_f64())
    .fetch_all(conn)
    .await
}

/// Claims the renewal of the person's authorization of the connector for
/// `hold`, if it is due within `within`, renewable, not claimed already,
/// and the connector enabled unless it is for a test.
pub async fn claim_person_renewal(
    conn: &mut PgConnection,
    connector_id: Uuid,
    user_id: Uuid,
    within: std::time::Duration,
    hold: std::time::Duration,
    asker: RenewalFor,
) -> sqlx::Result<Option<Claimed>> {
    // The person's own alone.
    let sql = format!("update {PERSON} {CLAIM} and user_id = $5 {CLAIMED}");
    let row = claim_query(&sql, connector_id, within, hold, asker)
        .bind(user_id)
        .fetch_optional(conn)
        .await?;
    Ok(row.map(Claimed::from))
}

/// Keeps a renewal of the person's authorization claimed, if it is still
/// that one; returns whether it was kept.
pub async fn keep_person_renewal(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
    renewed: Renewed<'_>,
) -> sqlx::Result<bool> {
    Ok(keep_in(conn, PERSON, claimed, renewed).await?.is_some())
}

/// Says on the person's authorization claimed why its renewal failed, if it
/// is still that one; returns whether it was said. The claim is left to end.
pub async fn record_person_renewal_failure(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
    reason: &str,
) -> sqlx::Result<bool> {
    say_failure_in(conn, PERSON, claimed, reason).await
}

/// Removes the person's authorization claimed, the server no longer
/// honouring it, if it is still that one; returns whether it was removed.
pub async fn remove_claimed_person_authorization(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
) -> sqlx::Result<bool> {
    Ok(remove_claimed_from(conn, PERSON, claimed).await?.is_some())
}
