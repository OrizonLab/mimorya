//! The roles model calls are recorded with (ADR-207), read back by tests.

use mimorya_core::demo::tenant;
use uuid::Uuid;

use super::TestApi;

/// The roles of the model calls `run` recorded, in order.
pub async fn recorded_roles(api: &TestApi, run: Uuid) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select role from usage_records where run_id = $1 and kind = 'llm' order by id",
    )
    .bind(run)
    .fetch_all(&mut *tx)
    .await
    .unwrap()
}

/// How many model calls of `role` the demo company recorded outside any
/// run (an evaluation, a connection test).
pub async fn calls_outside_runs(api: &TestApi, role: &str) -> i64 {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query_scalar(
        "select count(*) from usage_records where run_id is null and kind = 'llm' and role = $1",
    )
    .bind(role)
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}
