//! Sub-agents' runs (ADR-098): opened beside the run that handed them a
//! task, of the same company and user as it; listed with it; failed when
//! it is taken up without them. One that its parent leaves by ending fails
//! with it (a trigger sees to it, `0041_sub_agents.sql`).

use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::runs::{RUN_COLUMNS, RunRow};

#[derive(Debug, Clone)]
pub struct NewSubRun {
    pub parent_run_id: Uuid,
    /// A sub-agent's profile (`research`, `analysis`, `planning`, `quality`).
    pub profile: &'static str,
    pub objective: String,
    pub budget: Value,
    /// The permissions it may use at most (`None`: its user's).
    pub allowed_permissions: Option<Vec<String>>,
}

/// Opens a child run of `parent_run_id`, running, read-only, for the same
/// company and user; `None` when the parent is not found.
pub async fn create_sub_run(
    conn: &mut PgConnection,
    input: NewSubRun,
) -> sqlx::Result<Option<RunRow>> {
    sqlx::query_as(&format!(
        "insert into agent_runs (tenant_id, parent_run_id, agent_profile, initiated_by_user_id, \
         on_behalf_of_user_id, objective, autonomy_level, allowed_permissions, budget, channel, \
         status, started_at) \
         select parent.tenant_id, parent.id, $2, parent.initiated_by_user_id, \
         parent.on_behalf_of_user_id, $3, 'read_only', $4, $5, parent.channel, 'running', now() \
         from agent_runs parent where parent.id = $1 \
         returning {RUN_COLUMNS}"
    ))
    .bind(input.parent_run_id)
    .bind(input.profile)
    .bind(&input.objective)
    .bind(&input.allowed_permissions)
    .bind(&input.budget)
    .fetch_optional(conn)
    .await
}

/// The children of `parent_run_id`, in the order they were opened.
pub async fn list_sub_runs(
    conn: &mut PgConnection,
    parent_run_id: Uuid,
) -> sqlx::Result<Vec<RunRow>> {
    sqlx::query_as(&format!(
        "select {RUN_COLUMNS} from agent_runs where parent_run_id = $1 order by created_at, id"
    ))
    .bind(parent_run_id)
    .fetch_all(conn)
    .await
}

/// Fails the children of `parent_run_id` still running, for `error`; their
/// identifiers.
pub async fn fail_running_sub_runs(
    conn: &mut PgConnection,
    parent_run_id: Uuid,
    error: &str,
) -> sqlx::Result<Vec<Uuid>> {
    sqlx::query_scalar(
        "update agent_runs set status = 'failed', error = $2, finished_at = now(), \
         updated_at = now() where parent_run_id = $1 and status = 'running' returning id",
    )
    .bind(parent_run_id)
    .bind(error)
    .fetch_all(conn)
    .await
}
