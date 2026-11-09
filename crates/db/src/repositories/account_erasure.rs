//! An account erased (ADR-189): what is checked of it, its row locked, and
//! its deletion, which takes with it what every table keeps on the person
//! (each key naming an account deletes its rows or empties its column).
//! The audit log keeps the identifier it names, unchanged (ADR-178). Read
//! and written in the person's company's transaction.

use sqlx::PgConnection;
use uuid::Uuid;

/// What an erasure checks of an account, its row locked until the end of
/// the transaction.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ErasureCandidateRow {
    pub status: String,
    pub email: String,
    pub live_runs: i64,
    pub awaited_decisions: i64,
    pub stand_ins: i64,
    pub led_units: i64,
}

/// What an erasure took, by its counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub struct ErasedCounts {
    pub conversations: i64,
    pub messages: i64,
    pub memories: i64,
    pub runs: i64,
}

/// The account `user_id`, locked, and what keeps it from being erased;
/// `None` when the company holds no such account.
pub async fn erasure_candidate(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<ErasureCandidateRow>> {
    sqlx::query_as(
        "select u.status, u.email::text as email, \
         (select count(*) from agent_runs r where r.tenant_id = u.tenant_id \
           and r.on_behalf_of_user_id = u.id \
           and r.status in ('queued', 'running', 'awaiting_approval')) as live_runs, \
         (select count(*) from approvals a where a.tenant_id = u.tenant_id \
           and a.approver_user_id = u.id and a.status = 'pending') as awaited_decisions, \
         (select count(*) from approval_delegations d where d.tenant_id = u.tenant_id \
           and d.delegate_user_id = u.id and d.ends_at > now()) as stand_ins, \
         (select count(*) from org_units o where o.tenant_id = u.tenant_id \
           and o.lead_user_id = u.id) as led_units \
         from users u where u.id = $1 for update of u",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// Deletes the account `user_id` and, through the keys naming it, what the
/// platform keeps on the person; what went, counted before. `None` when the
/// company holds no such account.
pub async fn erase_account(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Option<ErasedCounts>> {
    // One statement: the counts read the rows as they were before it.
    sqlx::query_as(
        "with gone as (delete from users where id = $1 returning id) \
         select \
         (select count(*) from conversations where owner_user_id = $1) as conversations, \
         (select count(*) from messages m join conversations c \
           on c.tenant_id = m.tenant_id and c.id = m.conversation_id \
           where c.owner_user_id = $1) as messages, \
         (select count(*) from memory_items where scope = 'user' \
           and owner_user_id = $1) as memories, \
         (select count(*) from agent_runs where on_behalf_of_user_id = $1) as runs \
         from gone",
    )
    .bind(user_id)
    .fetch_optional(conn)
    .await
}
