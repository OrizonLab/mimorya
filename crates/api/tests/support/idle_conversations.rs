//! The conversation retention tests' data (ADR-186), written as an operator
//! writes it: conversations idle for any time, with their messages, a run
//! and what it leaves (an event, a tool call, a validation, a sub-agent's
//! run), an audit entry naming it as the application writes one, and what
//! is left of them read back.

use mimorya_db::Database;
use uuid::Uuid;

use super::retention::owner;

/// A conversation of `person` in the company `tenant_id`, titled `title`:
/// a question and, a second later, its answer; its last activity `months`
/// ago.
pub async fn conversation(tenant_id: Uuid, person: Uuid, title: &str, months: i32) -> Uuid {
    let owner = owner().await;
    let id = sqlx::query_scalar(
        "insert into conversations (tenant_id, owner_user_id, title, created_at, updated_at) \
         values ($1, $2, $3, now() - make_interval(months => $4), \
         now() - make_interval(months => $4)) returning id",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(title)
    .bind(months)
    .fetch_one(&owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into messages (tenant_id, conversation_id, role, author_user_id, content, \
         created_at) values ($1, $2, 'user', $3, 'question', now()), \
         ($1, $2, 'assistant', null, 'réponse', now() + interval '1 second')",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(person)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    id
}

/// `count` empty conversations of `person` in the company `tenant_id`,
/// titled `prefix` and their rank, idle for `months`: a backlog.
pub async fn idle_backlog(tenant_id: Uuid, person: Uuid, prefix: &str, count: i32, months: i32) {
    let owner = owner().await;
    sqlx::query(
        "insert into conversations (tenant_id, owner_user_id, title, created_at, updated_at) \
         select $1, $2, $3 || ' ' || rank, now() - make_interval(months => $5), \
         now() - make_interval(months => $5) from generate_series(1, $4) as rank",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(prefix)
    .bind(count)
    .bind(months)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// A run of `person` in the conversation `conversation_id`, in `status`,
/// and what it leaves: an event, a tool call awaiting or given its
/// validation, and a sub-agent's run.
pub async fn run(tenant_id: Uuid, conversation_id: Uuid, person: Uuid, status: &str) -> Uuid {
    let owner = owner().await;
    let id: Uuid = sqlx::query_scalar(
        "insert into agent_runs (tenant_id, conversation_id, initiated_by_user_id, \
         on_behalf_of_user_id, objective, budget, status) \
         values ($1, $2, $3, $3, 'objectif', '{}', $4) returning id",
    )
    .bind(tenant_id)
    .bind(conversation_id)
    .bind(person)
    .bind(status)
    .fetch_one(&owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into run_events (tenant_id, run_id, seq, type, data) \
         values ($1, $2, 1, 'step', '{}')",
    )
    .bind(tenant_id)
    .bind(id)
    .execute(&owner)
    .await
    .unwrap();
    let pending = status == "awaiting_approval";
    sqlx::query(
        "insert into approvals (tenant_id, run_id, requested_for_user_id, tool_name, \
         tool_call_id, action_summary, payload, risk, reason, status) \
         values ($1, $2, $3, 'demo_tool', 'appel-1', 'Action', '{}', 'write', 'raison', \
         case when $4 then 'pending' else 'approved' end)",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(person)
    .bind(pending)
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into tool_invocations (tenant_id, run_id, tool_call_id, tool_name, decision, \
         status) values ($1, $2, 'appel-1', 'demo_tool', 'require_approval', \
         case when $3 then 'awaiting_approval' else 'succeeded' end)",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(pending)
    .execute(&owner)
    .await
    .unwrap();
    sqlx::query(
        "insert into agent_runs (tenant_id, parent_run_id, on_behalf_of_user_id, \
         agent_profile, objective, budget, status) \
         values ($1, $2, $3, 'research', 'recherche', '{}', 'completed')",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(person)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    id
}

/// An entry of the company `tenant_id` naming the run `run_id`, written as
/// the application writes one.
pub async fn audited_run(db: &Database, tenant_id: Uuid, run_id: Uuid) -> i64 {
    let mut tx = db.begin(tenant_id).await.unwrap();
    let id = sqlx::query_scalar(
        "insert into audit_log (tenant_id, actor_type, run_id, action, outcome) \
         values ($1, 'agent', $2, 'tool.executed', 'success') returning id",
    )
    .bind(tenant_id)
    .bind(run_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id
}

/// What is left of the run `run_id`: itself and its sub-agents' runs, its
/// events, tool calls and validations.
pub async fn left_of_run(run_id: Uuid) -> i64 {
    let owner = owner().await;
    let left = sqlx::query_scalar(
        "select (select count(*) from agent_runs where id = $1 or parent_run_id = $1) \
              + (select count(*) from run_events where run_id = $1) \
              + (select count(*) from tool_invocations where run_id = $1) \
              + (select count(*) from approvals where run_id = $1)",
    )
    .bind(run_id)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    left
}

/// The titles of the conversations left that start with `prefix`, sorted,
/// and how many messages they hold.
pub async fn conversations_left(prefix: &str) -> (Vec<String>, i64) {
    let owner = owner().await;
    let mut titles: Vec<String> =
        sqlx::query_scalar("select title from conversations where title like $1 || '%'")
            .bind(prefix)
            .fetch_all(&owner)
            .await
            .unwrap();
    let messages = sqlx::query_scalar(
        "select count(*) from messages m join conversations c on c.id = m.conversation_id \
         where c.title like $1 || '%'",
    )
    .bind(prefix)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    titles.sort();
    (titles, messages)
}

/// The run the audit entry `id` names.
pub async fn named_run(id: i64) -> Option<Uuid> {
    let owner = owner().await;
    let run = sqlx::query_scalar("select run_id from audit_log where id = $1")
        .bind(id)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    run
}
