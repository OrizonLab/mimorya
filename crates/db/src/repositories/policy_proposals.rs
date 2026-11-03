//! Allow policies proposed by one administrator, decided by another
//! (ADR-168). A proposal states the policy as it will stand; it is decided
//! once, while pending, and the service writes the policy it approves
//! (`access_policies.rs`). Past its expiry nobody approves it.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Where a proposal stands (`policy_proposals_status_check`); a pending one
/// past its expiry is shown as expired, never stored as such.
pub const PROPOSAL_STATUSES: [&str; 4] = ["pending", "approved", "rejected", "withdrawn"];

/// How many proposals may wait for a decision in a company at once, none
/// expired: the administration lists them all.
pub const MAX_PENDING_PROPOSALS: i64 = 50;

/// How many proposals the administration lists: every one waiting for a
/// decision first, then the latest decided or expired.
pub const LISTED_PROPOSALS: i64 = MAX_PENDING_PROPOSALS + 20;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProposalRow {
    pub id: Uuid,
    /// The allow policy changed; `None` for a new one.
    pub policy_id: Option<Uuid>,
    /// That policy's last change when proposed.
    pub policy_updated_at: Option<DateTime<Utc>>,
    pub description: String,
    pub permissions: Vec<String>,
    pub condition: Value,
    pub enabled: bool,
    pub proposed_by_user_id: Uuid,
    pub proposed_by_name: String,
    pub proposed_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: String,
    pub decided_by_user_id: Option<Uuid>,
    pub decided_by_name: Option<String>,
    pub decided_at: Option<DateTime<Utc>>,
    /// Pending, its policy changed since: it can no longer be approved.
    pub outdated: bool,
}

const PROPOSAL_SELECT: &str = "select p.id, p.policy_id, p.policy_updated_at, p.description, \
    p.permissions, p.condition, p.enabled, p.proposed_by_user_id, \
    author.display_name as proposed_by_name, p.proposed_at, p.expires_at, p.status, \
    p.decided_by_user_id, decider.display_name as decided_by_name, p.decided_at, \
    (p.status = 'pending' and coalesce(policy.updated_at <> p.policy_updated_at, false)) \
      as outdated \
    from policy_proposals p \
    join users author on author.tenant_id = p.tenant_id and author.id = p.proposed_by_user_id \
    left join users decider on decider.tenant_id = p.tenant_id \
      and decider.id = p.decided_by_user_id \
    left join access_policies policy on policy.tenant_id = p.tenant_id \
      and policy.id = p.policy_id";

#[derive(Debug, Clone)]
pub struct NewProposal {
    pub tenant_id: Uuid,
    pub policy_id: Option<Uuid>,
    pub policy_updated_at: Option<DateTime<Utc>>,
    pub description: String,
    pub permissions: Vec<String>,
    /// A serialized `mimorya_core::authz::Condition`, checked by the caller.
    pub condition: Value,
    pub enabled: bool,
    pub proposed_by_user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_proposal(conn: &mut PgConnection, input: NewProposal) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into policy_proposals (tenant_id, policy_id, policy_updated_at, description, \
         permissions, condition, enabled, proposed_by_user_id, expires_at) \
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning id",
    )
    .bind(input.tenant_id)
    .bind(input.policy_id)
    .bind(input.policy_updated_at)
    .bind(input.description)
    .bind(input.permissions)
    .bind(input.condition)
    .bind(input.enabled)
    .bind(input.proposed_by_user_id)
    .bind(input.expires_at)
    .fetch_one(conn)
    .await
}

/// Every proposal waiting for a decision, the latest first, then the latest
/// decided or expired, up to `LISTED_PROPOSALS` in all.
pub async fn list_proposals(conn: &mut PgConnection) -> sqlx::Result<Vec<ProposalRow>> {
    sqlx::query_as(&format!(
        "{PROPOSAL_SELECT} order by (p.status = 'pending' and p.expires_at > now()) desc, \
         coalesce(p.decided_at, \
           case when p.expires_at > now() then p.proposed_at else p.expires_at end) desc, \
         p.id limit $1"
    ))
    .bind(LISTED_PROPOSALS)
    .fetch_all(conn)
    .await
}

/// How many proposals of the policy `policy_id` wait for a decision.
pub async fn count_policy_proposals(conn: &mut PgConnection, policy_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select count(*) from policy_proposals where policy_id = $1 and status = 'pending'",
    )
    .bind(policy_id)
    .fetch_one(conn)
    .await
}

/// How many proposals wait for a decision, none expired.
pub async fn count_waiting_proposals(conn: &mut PgConnection) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select count(*) from policy_proposals where status = 'pending' and expires_at > now()",
    )
    .fetch_one(conn)
    .await
}

pub async fn get_proposal(conn: &mut PgConnection, id: Uuid) -> sqlx::Result<Option<ProposalRow>> {
    sqlx::query_as(&format!("{PROPOSAL_SELECT} where p.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

/// Decides the pending proposal `id` (`approved`, `rejected` or
/// `withdrawn`) by `user_id`: returns whether it was pending.
pub async fn decide_proposal(
    conn: &mut PgConnection,
    id: Uuid,
    status: &str,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "update policy_proposals set status = $2, decided_by_user_id = $3, decided_at = now() \
         where id = $1 and status = 'pending'",
    )
    .bind(id)
    .bind(status)
    .bind(user_id)
    .execute(conn)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Deletes the proposals of every company decided, or left to expire,
/// before `before`; how many.
pub async fn purge_policy_proposals(pool: &PgPool, before: DateTime<Utc>) -> sqlx::Result<i32> {
    sqlx::query_scalar("select system_purge_policy_proposals($1)")
        .bind(before)
        .fetch_one(pool)
        .await
}
