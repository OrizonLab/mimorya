//! The retention tests' data (ADR-185), written as an operator writes it:
//! accounts, memories and sessions of any age, the date of an account's
//! disabling moved back, past the rule that keeps it the database's, and
//! what is left of them read back.

use sqlx::PgPool;
use uuid::Uuid;

use super::owner_url;

pub async fn owner() -> PgPool {
    PgPool::connect(owner_url()).await.unwrap()
}

/// An account of the company `tenant_id` at `email`, `status`, as an
/// operator writes it.
pub async fn account(tenant_id: Uuid, email: &str, status: &str) -> Uuid {
    let owner = owner().await;
    let id = sqlx::query_scalar(
        "insert into users (tenant_id, email, display_name, status) \
         values ($1, $2, 'Compte de la rétention', $3) returning id",
    )
    .bind(tenant_id)
    .bind(email)
    .bind(status)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    id
}

/// When the database says the account `id` was disabled.
pub async fn disabled_at(id: Uuid) -> Option<chrono::DateTime<chrono::Utc>> {
    let owner = owner().await;
    let at = sqlx::query_scalar("select disabled_at from users where id = $1")
        .bind(id)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    at
}

/// Moves the disabling of the account `id` `days` back, past the rule that
/// keeps it the database's: what time does.
pub async fn disabled_days_ago(id: Uuid, days: i32) {
    let owner = owner().await;
    let mut tx = owner.begin().await.unwrap();
    sqlx::query("alter table users disable trigger users_disabled_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("update users set disabled_at = now() - make_interval(days => $2) where id = $1")
        .bind(id)
        .bind(days)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("alter table users enable trigger users_disabled_at")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    owner.close().await;
}

/// A memory of the company `tenant_id` titled `title`: of `kind`, personal
/// to `owner_user_id` when it names one, recorded `days` ago, past its own
/// term when `expired`.
pub async fn memory(
    tenant_id: Uuid,
    title: &str,
    kind: &str,
    owner_user_id: Option<Uuid>,
    days: i32,
    expired: bool,
) {
    let owner = owner().await;
    sqlx::query(
        "insert into memory_items (tenant_id, scope, owner_user_id, kind, title, content, \
         source, created_at, expires_at) \
         values ($1, case when $2::uuid is null then 'tenant' else 'user' end, $2, $3, $4, \
         'contenu', '{}', now() - make_interval(days => $5), \
         case when $6 then now() - interval '1 minute' end)",
    )
    .bind(tenant_id)
    .bind(owner_user_id)
    .bind(kind)
    .bind(title)
    .bind(days)
    .bind(expired)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// The titles of the memories starting with `prefix`, sorted.
pub async fn titles(prefix: &str) -> Vec<String> {
    let owner = owner().await;
    let mut titles: Vec<String> =
        sqlx::query_scalar("select title from memory_items where title like $1 || '%'")
            .bind(prefix)
            .fetch_all(&owner)
            .await
            .unwrap();
    owner.close().await;
    titles.sort();
    titles
}

/// A session of the account `user_id` of the company `tenant_id`, known by
/// its user agent `agent`: revoked, idle past its term and past its
/// absolute term the days from now each says (`None`: never revoked).
pub async fn session(
    tenant_id: Uuid,
    user_id: Uuid,
    agent: &str,
    revoked: Option<i32>,
    expires: i32,
    absolute: i32,
) {
    let owner = owner().await;
    sqlx::query(
        "insert into sessions (tenant_id, user_id, token_hash, expires_at, \
         absolute_expires_at, revoked_at, user_agent) \
         values ($1, $2, md5($3), now() + make_interval(days => $4), \
         now() + make_interval(days => $5), now() + make_interval(days => $6), $3)",
    )
    .bind(tenant_id)
    .bind(user_id)
    .bind(agent)
    .bind(expires)
    .bind(absolute)
    .bind(revoked)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}

/// The user agents of the sessions left that start with `prefix`, sorted.
pub async fn session_agents(prefix: &str) -> Vec<String> {
    let owner = owner().await;
    let mut agents: Vec<String> =
        sqlx::query_scalar("select user_agent from sessions where user_agent like $1 || '%'")
            .bind(prefix)
            .fetch_all(&owner)
            .await
            .unwrap();
    owner.close().await;
    agents.sort();
    agents
}

/// The metadata of the company `tenant_id`'s entries of `action` written
/// by the platform (`memory.purged`, `conversation.purged`, `usage.purged`).
pub async fn purged_entries(tenant_id: Uuid, action: &str) -> Vec<serde_json::Value> {
    let owner = owner().await;
    let entries = sqlx::query_scalar(
        "select metadata from audit_log where tenant_id = $1 and action = $2 \
         and actor_type = 'system' order by id",
    )
    .bind(tenant_id)
    .bind(action)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    entries
}

/// Moves the last change of the company `tenant_id`'s retention duration
/// of `kind` `days` back, past the rule that keeps it the database's: what
/// time does.
pub async fn retention_changed_days_ago(tenant_id: Uuid, kind: &str, days: i32) {
    let owner = owner().await;
    let mut tx = owner.begin().await.unwrap();
    sqlx::query(
        "alter table tenant_retention_durations disable trigger tenant_retention_durations_changed",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "update tenant_retention_durations set changed_at = now() - make_interval(days => $3) \
         where tenant_id = $1 and kind = $2",
    )
    .bind(tenant_id)
    .bind(kind)
    .bind(days)
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query(
        "alter table tenant_retention_durations enable trigger tenant_retention_durations_changed",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    owner.close().await;
}

/// The company `tenant_id` back to the platform's durations, as an operator
/// removes them: what a test leaves.
pub async fn clear_retention(tenant_id: Uuid) {
    let owner = owner().await;
    sqlx::query("delete from tenant_retention_durations where tenant_id = $1")
        .bind(tenant_id)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}
