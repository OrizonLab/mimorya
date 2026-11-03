//! What tells the other administrators of the access policies of the allow
//! policies proposed (ADR-170): the companies holding proposals still
//! waiting and never announced, those proposals with their author, and the
//! record that they were told, or that the relay refused it.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// A proposal still waiting (pending, not expired), never announced.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UnannouncedPolicyProposal {
    pub id: Uuid,
    pub proposed_by_user_id: Uuid,
    pub proposed_by_name: String,
}

/// Every active company holding proposals still waiting and never
/// announced, through a narrowly scoped database function: each is then
/// read in its own company.
pub async fn companies_with_unannounced_policy_proposals(pool: &PgPool) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar("select tenant_id from system_unannounced_policy_proposals()")
        .fetch_all(pool)
        .await
}

/// The company's proposals still waiting and never announced, oldest first.
pub async fn unannounced_policy_proposals(
    conn: &mut PgConnection,
) -> sqlx::Result<Vec<UnannouncedPolicyProposal>> {
    sqlx::query_as(
        "select p.id, p.proposed_by_user_id, u.display_name as proposed_by_name \
         from policy_proposals p \
         join users u on u.tenant_id = p.tenant_id and u.id = p.proposed_by_user_id \
         where p.status = 'pending' and p.expires_at > now() and p.announced_at is null \
         order by p.proposed_at, p.id",
    )
    .fetch_all(conn)
    .await
}

/// Records that `ids` were announced, or that the relay refused their
/// notice: none is announced again.
pub async fn record_policy_proposals_announced(
    conn: &mut PgConnection,
    ids: &[Uuid],
) -> sqlx::Result<()> {
    sqlx::query(
        "update policy_proposals set announced_at = now() \
         where id = any($1) and announced_at is null",
    )
    .bind(ids)
    .execute(conn)
    .await?;
    Ok(())
}
