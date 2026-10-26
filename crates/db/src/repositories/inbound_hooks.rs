//! Inbound webhooks a person sets for their agent (ADR-109): read and
//! written by their author only, their token kept as a digest; a delivery
//! finds its company by the hook and its token, then counts in the hook's
//! hourly window, the row locked.

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct InboundHookRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub instruction: String,
    pub enabled: bool,
    pub window_started_at: Option<DateTime<Utc>>,
    pub window_deliveries: i32,
    pub last_received_at: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
    pub last_problem: Option<String>,
    pub last_conversation_id: Option<Uuid>,
    pub last_run_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const COLUMNS: &str = "h.id, h.user_id, h.title, h.instruction, h.enabled, \
     h.window_started_at, h.window_deliveries, h.last_received_at, h.last_outcome, \
     h.last_problem, h.last_conversation_id, h.last_run_id, h.created_at, h.updated_at";

/// What a hook asks, and whether it receives.
pub struct HookDefinition<'a> {
    pub title: &'a str,
    pub instruction: &'a str,
    pub enabled: bool,
}

/// The hooks of `user_id`, oldest first.
pub async fn list_inbound_hooks(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> sqlx::Result<Vec<InboundHookRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from inbound_hooks h where h.user_id = $1 order by h.created_at, h.id"
    ))
    .bind(user_id)
    .fetch_all(conn)
    .await
}

/// The hook `id` of `user_id`, locked against another change.
pub async fn lock_inbound_hook(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<Option<InboundHookRow>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from inbound_hooks h where h.id = $1 and h.user_id = $2 for update"
    ))
    .bind(id)
    .bind(user_id)
    .fetch_optional(conn)
    .await
}

/// How many hooks `user_id` has, under a lock of theirs until the
/// transaction ends: two creations at once count each other.
pub async fn count_inbound_hooks(conn: &mut PgConnection, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text, 109))")
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query_scalar("select count(*) from inbound_hooks where user_id = $1")
        .bind(user_id)
        .fetch_one(conn)
        .await
}

pub async fn create_inbound_hook(
    conn: &mut PgConnection,
    tenant_id: Uuid,
    user_id: Uuid,
    hook: HookDefinition<'_>,
    token_hash: &str,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "insert into inbound_hooks (tenant_id, user_id, title, instruction, enabled, \
         token_hash) values ($1, $2, $3, $4, $5, $6) returning id",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(hook.title)
    .bind(hook.instruction)
    .bind(hook.enabled)
    .bind(token_hash)
    .fetch_one(conn)
    .await
}

/// Rewrites the hook `id` (locked first by `lock_inbound_hook`).
pub async fn update_inbound_hook(
    conn: &mut PgConnection,
    id: Uuid,
    hook: HookDefinition<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update inbound_hooks set title = $2, instruction = $3, enabled = $4, \
         updated_at = now() where id = $1",
    )
    .bind(id)
    .bind(hook.title)
    .bind(hook.instruction)
    .bind(hook.enabled)
    .execute(conn)
    .await?;
    Ok(())
}

/// Replaces the token of the hook `id`: the former one no longer delivers.
pub async fn replace_hook_token(
    conn: &mut PgConnection,
    id: Uuid,
    token_hash: &str,
) -> sqlx::Result<()> {
    sqlx::query("update inbound_hooks set token_hash = $2, updated_at = now() where id = $1")
        .bind(id)
        .bind(token_hash)
        .execute(conn)
        .await?;
    Ok(())
}

/// Deletes the hook `id` of `user_id`; whether there was one.
pub async fn delete_inbound_hook(
    conn: &mut PgConnection,
    id: Uuid,
    user_id: Uuid,
) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from inbound_hooks where id = $1 and user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() > 0)
}

/// The company of the enabled hook `id` whose token digest is `token_hash`,
/// before any session; `None` for an unknown hook, a wrong token or a paused
/// hook alike.
pub async fn inbound_hook_tenant(
    pool: &PgPool,
    id: Uuid,
    token_hash: &str,
) -> sqlx::Result<Option<Uuid>> {
    sqlx::query_scalar("select auth_inbound_hook_tenant($1, $2)")
        .bind(id)
        .bind(token_hash)
        .fetch_one(pool)
        .await
}

/// A hook a delivery reached, locked, with what its author's account
/// allows now.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReachedHook {
    #[sqlx(flatten)]
    pub hook: InboundHookRow,
    pub owner_status: String,
    pub must_change_password: bool,
}

/// The enabled hook `id` whose token digest is `token_hash`, locked: the
/// deliveries of one hook are counted one at a time.
pub async fn lock_reached_hook(
    conn: &mut PgConnection,
    id: Uuid,
    token_hash: &str,
) -> sqlx::Result<Option<ReachedHook>> {
    sqlx::query_as(&format!(
        "select {COLUMNS}, u.status as owner_status, u.must_change_password \
         from inbound_hooks h join users u on u.id = h.user_id \
         where h.id = $1 and h.token_hash = $2 and h.enabled for update of h"
    ))
    .bind(id)
    .bind(token_hash)
    .fetch_optional(conn)
    .await
}

/// Counts a delivery in the window starting at `window_started_at`.
pub async fn count_delivery(
    conn: &mut PgConnection,
    id: Uuid,
    window_started_at: DateTime<Utc>,
    window_deliveries: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "update inbound_hooks set window_started_at = $2, window_deliveries = $3 where id = $1",
    )
    .bind(id)
    .bind(window_started_at)
    .bind(window_deliveries)
    .execute(conn)
    .await?;
    Ok(())
}

/// What became of a delivery.
pub struct Delivery<'a> {
    pub at: DateTime<Utc>,
    pub outcome: &'a str,
    pub problem: Option<&'a str>,
    pub conversation_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
}

/// Keeps what became of the last delivery of the hook `id`, for its author.
pub async fn record_delivery(
    conn: &mut PgConnection,
    id: Uuid,
    delivery: Delivery<'_>,
) -> sqlx::Result<()> {
    sqlx::query(
        "update inbound_hooks set last_received_at = $2, last_outcome = $3, \
         last_problem = $4, last_conversation_id = coalesce($5, last_conversation_id), \
         last_run_id = coalesce($6, last_run_id) where id = $1",
    )
    .bind(id)
    .bind(delivery.at)
    .bind(delivery.outcome)
    .bind(delivery.problem)
    .bind(delivery.conversation_id)
    .bind(delivery.run_id)
    .execute(conn)
    .await?;
    Ok(())
}
