//! The requests awaiting a manager announced in Slack (ADR-130), in
//! Telegram (ADR-137) or in Teams (ADR-142): the manager, or whom they
//! named to stand in for them, linked to the company's Slack application,
//! Telegram bot or Teams bot, is written a message of a paused run's
//! requests awaiting them, by the periodic pass, within a while of them,
//! once through each gateway. What became of each is kept, never what was
//! written: that is the request itself.

use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::approval_notices::ApprovalNotice;
use super::gateway_replies::ReplyRecord;

/// A paused run, in some company, with requests to announce through one
/// of its gateways.
#[derive(Debug, Clone, Copy, sqlx::FromRow)]
pub struct RunToNotify {
    pub tenant_id: Uuid,
    pub run_id: Uuid,
    pub gateway_id: Uuid,
}

/// Every company's paused runs with requests made since `since` awaiting a
/// manager, their notice through an enabled gateway still to try at `now`,
/// with that gateway, the oldest first, at most `limit`, through a narrowly
/// scoped database function: each is then told of in its own company.
pub async fn gateway_approvals_to_notify(
    pool: &PgPool,
    since: DateTime<Utc>,
    now: DateTime<Utc>,
    limit: i32,
) -> sqlx::Result<Vec<RunToNotify>> {
    sqlx::query_as(
        "select tenant_id, run_id, gateway_id \
         from system_gateway_approvals_to_notify($1, $2, $3)",
    )
    .bind(since)
    .bind(now)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// A request awaiting a manager, to announce through a gateway.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GatewayApprovalNotice {
    #[sqlx(flatten)]
    pub notice: ApprovalNotice,
    /// What the agent says it does.
    pub action_summary: String,
    /// Its exact parameters.
    pub payload: Value,
    /// The attempts made so far, through this gateway.
    pub attempts: i32,
}

/// The requests of `run_id` made since `since` awaiting a manager whose
/// notice through `gateway_id` is to try at `now`: pending, the run paused
/// and its company active, the manager's account active; by manager, then
/// in creation order.
pub async fn gateway_approval_notices(
    conn: &mut PgConnection,
    (run_id, gateway_id): (Uuid, Uuid),
    since: DateTime<Utc>,
    now: DateTime<Utc>,
) -> sqlx::Result<Vec<GatewayApprovalNotice>> {
    sqlx::query_as(
        "select a.id as approval_id, a.run_id, a.risk, a.expires_at, \
         a.requested_for_user_id, r.display_name as requested_for_name, a.approver_user_id, \
         m.email::text as approver_email, m.display_name as approver_name, t.name as company, \
         a.action_summary, a.payload, coalesce(n.attempts, 0) as attempts \
         from approvals a \
         join agent_runs ar on ar.id = a.run_id \
         join users r on r.id = a.requested_for_user_id \
         join users m on m.id = a.approver_user_id \
         join tenants t on t.id = a.tenant_id \
         left join gateway_approval_notices n on n.approval_id = a.id and n.gateway_id = $2 \
         where a.run_id = $1 and a.status = 'pending' and a.routing = 'manager' \
         and a.created_at > $3 \
         and ar.status = 'awaiting_approval' and t.status = 'active' and m.status = 'active' \
         and (n.approval_id is null or (n.outcome is null and n.retry_after <= $4)) \
         order by a.approver_user_id, a.created_at",
    )
    .bind(run_id)
    .bind(gateway_id)
    .bind(since)
    .bind(now)
    .fetch_all(conn)
    .await
}

/// Records what became of the notice of `approval_id` through
/// `gateway_id`, meant for `user_id`, one attempt more.
pub async fn record_gateway_approval_notice(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    approval_id: Uuid,
    (gateway_id, user_id): (Uuid, Uuid),
    record: ReplyRecord,
) -> sqlx::Result<()> {
    let (outcome, retry_after) = match record {
        ReplyRecord::Done(outcome) => (Some(outcome), None),
        ReplyRecord::Retry(at) => (None, Some(at)),
    };
    sqlx::query(
        "insert into gateway_approval_notices \
         (approval_id, tenant_id, gateway_id, user_id, outcome, retry_after) \
         values ($1, $2, $3, $4, $5, $6) on conflict (approval_id, gateway_id) do update \
         set user_id = excluded.user_id, outcome = excluded.outcome, \
         retry_after = excluded.retry_after, \
         attempts = gateway_approval_notices.attempts + 1, updated_at = now()",
    )
    .bind(approval_id)
    .bind(tenant_id)
    .bind(gateway_id)
    .bind(user_id)
    .bind(outcome)
    .bind(retry_after)
    .execute(conn)
    .await?;
    Ok(())
}
