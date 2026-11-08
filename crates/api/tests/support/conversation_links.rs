//! What names a conversation, its runs and its messages, written as an
//! operator writes it for the retention tests (ADR-186): what goes with the
//! conversation (its mission, its summary, its Slack thread and the card
//! written there), what keeps its row and forgets it (the scheduled task
//! and the trigger that last opened it, the usage, memory and deliverable
//! its run left), and all of it read back.

use sqlx::PgPool;
use uuid::Uuid;

use super::retention::owner;

/// What was written around a conversation.
pub struct Links {
    pub task: Uuid,
    pub hook: Uuid,
    pub mission: Uuid,
    pub thread: Uuid,
    pub usage: i64,
    pub memory: Uuid,
    pub deliverable: Uuid,
}

/// What is left of the links: the task's and the trigger's last
/// conversation and run; how many of the mission, summary, thread and card
/// are left; the run the usage, memory and deliverable name.
#[derive(Debug, PartialEq, Eq)]
pub struct LinksLeft {
    pub task: (Option<Uuid>, Option<Uuid>),
    pub hook: (Option<Uuid>, Option<Uuid>),
    pub gone_with_it: (i64, i64, i64, i64),
    pub runs_named: (Option<Uuid>, Option<Uuid>, Option<Uuid>),
}

/// Runs `sql`, bound to `ids` in order, returning its one identifier.
async fn insert(owner: &PgPool, sql: &str, ids: &[Uuid]) -> Uuid {
    let mut query = sqlx::query_scalar(sql);
    for id in ids {
        query = query.bind(*id);
    }
    query.fetch_one(owner).await.unwrap()
}

/// Writes around the conversation `conversation` of `person`, its run
/// `run` (with its validation and tool call) and its two messages,
/// everything that names them.
pub async fn link(tenant_id: Uuid, person: Uuid, conversation: Uuid, run: Uuid) -> Links {
    let owner = owner().await;
    let ids = [tenant_id, person, conversation, run];
    // The run names its request; the answer, its run; the tool call, its
    // validation.
    insert(
        &owner,
        "update agent_runs set request_message_id = \
         (select id from messages where conversation_id = $2 and role = 'user') \
         where id = $1 returning id",
        &[run, conversation],
    )
    .await;
    insert(
        &owner,
        "update messages set run_id = $1 \
         where conversation_id = $2 and role = 'assistant' returning id",
        &[run, conversation],
    )
    .await;
    insert(
        &owner,
        "update tool_invocations set approval_id = (select id from approvals where run_id = $1) \
         where run_id = $1 returning id",
        &[run],
    )
    .await;
    let task = insert(
        &owner,
        "insert into scheduled_tasks (tenant_id, user_id, title, instruction, weekdays, \
         time_of_day, time_zone, last_conversation_id, last_run_id) \
         values ($1, $2, 'Tâche de la rétention', 'Faire le point', '{1}', '09:00', \
         'Europe/Paris', $3, $4) returning id",
        &ids,
    )
    .await;
    let hook = insert(
        &owner,
        "insert into inbound_hooks (tenant_id, user_id, title, instruction, token_hash, \
         last_conversation_id, last_run_id) \
         values ($1, $2, 'Déclencheur de la rétention', 'Lire l''envoi', repeat('a', 64), \
         $3, $4) returning id",
        &ids,
    )
    .await;
    let mission = insert(
        &owner,
        "insert into missions (tenant_id, owner_user_id, title, description, \
         conversation_id, limits) \
         values ($1, $2, 'Mission de la rétention', 'Une mission', $3, '{}') returning id",
        &ids[..3],
    )
    .await;
    insert(
        &owner,
        "insert into conversation_summaries (tenant_id, conversation_id, content, \
         through_message_id, folded_messages) \
         select $1, $2, 'Résumé', id, 1 from messages \
         where conversation_id = $2 and role = 'user' returning conversation_id",
        &[tenant_id, conversation],
    )
    .await;
    let gateway = insert(
        &owner,
        "insert into gateways (tenant_id, kind, name, workspace_id, workspace_name, \
         bot_user_id) values ($1, 'slack', 'Slack de la rétention', 'TRETENTION', \
         'Rétention', 'URETBOT') returning id",
        &[tenant_id],
    )
    .await;
    let thread = insert(
        &owner,
        "insert into gateway_threads (tenant_id, gateway_id, user_id, conversation_id, kind, \
         external_id, channel_id, thread_ts) \
         values ($1, $2, $3, $4, 'slack', 'URETPERSON', 'DRETENTION', '1700000000.000100') \
         returning id",
        &[tenant_id, gateway, person, conversation],
    )
    .await;
    insert(
        &owner,
        "insert into gateway_cards (tenant_id, thread_id, gateway_id, channel_id, \
         activity_id, approval_ids) \
         select $1, $2, $3, 'DRETENTION', 'carte-1', array[a.id] from approvals a \
         where a.run_id = $4 returning id",
        &[tenant_id, thread, gateway, run],
    )
    .await;
    let usage = sqlx::query_scalar(
        "insert into usage_records (tenant_id, user_id, run_id, kind, provider, model) \
         values ($1, $2, $3, 'llm', 'anthropic', 'modèle') returning id",
    )
    .bind(tenant_id)
    .bind(person)
    .bind(run)
    .fetch_one(&owner)
    .await
    .unwrap();
    let memory = insert(
        &owner,
        "insert into memory_items (tenant_id, scope, owner_user_id, kind, title, content, \
         source, created_by_run_id) \
         values ($1, 'user', $2, 'fact', 'Mémoire de la rétention', 'contenu', '{}', $3) \
         returning id",
        &[tenant_id, person, run],
    )
    .await;
    let deliverable = insert(
        &owner,
        "insert into deliverables (tenant_id, owner_user_id, run_id, type, title, content) \
         values ($1, $2, $3, 'note', 'Livrable de la rétention', '{}') returning id",
        &[tenant_id, person, run],
    )
    .await;
    owner.close().await;
    Links {
        task,
        hook,
        mission,
        thread,
        usage,
        memory,
        deliverable,
    }
}

/// The last conversation and run of the row `id` of `table`.
async fn last(owner: &PgPool, table: &str, id: Uuid) -> (Option<Uuid>, Option<Uuid>) {
    sqlx::query_as(&format!(
        "select last_conversation_id, last_run_id from {table} where id = $1"
    ))
    .bind(id)
    .fetch_one(owner)
    .await
    .unwrap()
}

/// What is left of `links`, around the conversation `conversation`.
pub async fn left(links: &Links, conversation: Uuid) -> LinksLeft {
    let owner = owner().await;
    let task = last(&owner, "scheduled_tasks", links.task).await;
    let hook = last(&owner, "inbound_hooks", links.hook).await;
    let gone_with_it = sqlx::query_as(
        "select (select count(*) from missions where id = $1), \
                (select count(*) from conversation_summaries where conversation_id = $2), \
                (select count(*) from gateway_threads where id = $3), \
                (select count(*) from gateway_cards where thread_id = $3)",
    )
    .bind(links.mission)
    .bind(conversation)
    .bind(links.thread)
    .fetch_one(&owner)
    .await
    .unwrap();
    let runs_named = sqlx::query_as(
        "select (select run_id from usage_records where id = $1), \
                (select created_by_run_id from memory_items where id = $2), \
                (select run_id from deliverables where id = $3)",
    )
    .bind(links.usage)
    .bind(links.memory)
    .bind(links.deliverable)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    LinksLeft {
        task,
        hook,
        gone_with_it,
        runs_named,
    }
}
