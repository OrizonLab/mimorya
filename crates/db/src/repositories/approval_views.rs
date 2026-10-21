//! Approvals as shown to people: with the names of whom the agent acts for,
//! of the manager who decides (ADR-059) and of who decided, in one query.

use sqlx::PgConnection;
use uuid::Uuid;

use super::approvals::{APPROVAL_COLUMNS, ApprovalRow};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ApprovalView {
    #[sqlx(flatten)]
    pub approval: ApprovalRow,
    pub requested_for_name: String,
    pub approver_name: Option<String>,
    pub decided_by_name: Option<String>,
}

/// The selected approvals (`filter`, on the `approvals` columns), named.
fn named(filter: &str) -> String {
    format!(
        "select a.*, r.display_name as requested_for_name, m.display_name as approver_name, \
         d.display_name as decided_by_name \
         from (select {APPROVAL_COLUMNS} from approvals where {filter}) a \
         join users r on r.id = a.requested_for_user_id \
         left join users m on m.id = a.approver_user_id \
         left join users d on d.id = a.decided_by_user_id"
    )
}

/// Those the user asked for or must decide, newest first.
pub async fn list_approval_views_for_user(
    conn: &mut PgConnection,
    user_id: Uuid,
    statuses: &[&str],
) -> sqlx::Result<Vec<ApprovalView>> {
    let query = named(
        "(requested_for_user_id = $1 or approver_user_id = $1) and status = any($2) \
         order by created_at desc limit 100",
    );
    sqlx::query_as(&format!("{query} order by a.created_at desc"))
        .bind(user_id)
        .bind(statuses)
        .fetch_all(conn)
        .await
}

pub async fn list_approval_views_for_run(
    conn: &mut PgConnection,
    run_id: Uuid,
) -> sqlx::Result<Vec<ApprovalView>> {
    let query = named("run_id = $1");
    sqlx::query_as(&format!("{query} order by a.created_at"))
        .bind(run_id)
        .fetch_all(conn)
        .await
}

pub async fn get_approval_view(
    conn: &mut PgConnection,
    id: Uuid,
) -> sqlx::Result<Option<ApprovalView>> {
    sqlx::query_as(&named("id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await
}
