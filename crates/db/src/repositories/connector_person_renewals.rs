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
    Claimed, ClaimedRenewal, ClaimedRow, RenewalFor, Renewed, failure_said,
};

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
    let row: Option<ClaimedRow> = sqlx::query_as(
        "update connector_person_authorizations \
         set renewing_until = now() + make_interval(secs => $4) \
         where connector_id = $1 and user_id = $2 and refresh_ciphertext is not null \
         and access_expires_at < now() + make_interval(secs => $3) \
         and (renewing_until is null or renewing_until <= now()) \
         and ($5 or exists (select 1 from connector_instances i \
                            where i.id = connector_id and i.enabled)) \
         returning id, refreshed_at, issuer, client_id, resource, refresh_ciphertext, \
         refresh_iv, refresh_auth_tag, refresh_key_version",
    )
    .bind(connector_id)
    .bind(user_id)
    .bind(within.as_secs_f64())
    .bind(hold.as_secs_f64())
    .bind(asker == RenewalFor::Test)
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
    let refresh = renewed.refresh_token;
    let kept = sqlx::query(
        "update connector_person_authorizations set access_ciphertext = $3, access_iv = $4, \
         access_auth_tag = $5, access_key_version = $6, access_expires_at = $7, \
         refresh_ciphertext = coalesce($8, refresh_ciphertext), \
         refresh_iv = coalesce($9, refresh_iv), \
         refresh_auth_tag = coalesce($10, refresh_auth_tag), \
         refresh_key_version = coalesce($11, refresh_key_version), \
         scopes = coalesce($12, scopes), refreshed_at = now(), renewing_until = null, \
         renewal_failure = null \
         where id = $1 and refreshed_at = $2",
    )
    .bind(claimed.authorization_id)
    .bind(claimed.refreshed_at)
    .bind(&renewed.access_token.ciphertext)
    .bind(&renewed.access_token.iv)
    .bind(&renewed.access_token.auth_tag)
    .bind(renewed.access_token.key_version)
    .bind(renewed.access_expires_at)
    .bind(refresh.map(|r| r.ciphertext.clone()))
    .bind(refresh.map(|r| r.iv.clone()))
    .bind(refresh.map(|r| r.auth_tag.clone()))
    .bind(refresh.map(|r| r.key_version))
    .bind(renewed.scopes)
    .execute(conn)
    .await?;
    Ok(kept.rows_affected() == 1)
}

/// Says on the person's authorization claimed why its renewal failed, if it
/// is still that one; returns whether it was said. The claim is left to end.
pub async fn record_person_renewal_failure(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
    reason: &str,
) -> sqlx::Result<bool> {
    let said = sqlx::query(
        "update connector_person_authorizations set renewal_failure = $3 \
         where id = $1 and refreshed_at = $2",
    )
    .bind(claimed.authorization_id)
    .bind(claimed.refreshed_at)
    .bind(failure_said(reason))
    .execute(conn)
    .await?;
    Ok(said.rows_affected() == 1)
}

/// Removes the person's authorization claimed, the server no longer
/// honouring it, if it is still that one; returns whether it was removed.
pub async fn remove_claimed_person_authorization(
    conn: &mut PgConnection,
    claimed: ClaimedRenewal,
) -> sqlx::Result<bool> {
    let removed = sqlx::query(
        "delete from connector_person_authorizations where id = $1 and refreshed_at = $2",
    )
    .bind(claimed.authorization_id)
    .bind(claimed.refreshed_at)
    .execute(conn)
    .await?;
    Ok(removed.rows_affected() == 1)
}
