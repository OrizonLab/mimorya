//! A privilege of the application role taken away for one step, as a
//! failing database would, and given back whatever the step does: the tests
//! that follow in the same binary share the database.

use std::panic::{AssertUnwindSafe, resume_unwind};

use futures::FutureExt;

use super::owner_url;

/// Runs `step` while the application role may not insert into `table`.
pub async fn without_insert<T>(table: &str, step: impl Future<Output = T>) -> T {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let revoke = format!("revoke insert on {table} from mimorya_app");
    sqlx::query(&revoke).execute(&owner).await.unwrap();
    let outcome = AssertUnwindSafe(step).catch_unwind().await;
    let grant = format!("grant insert on {table} to mimorya_app");
    sqlx::query(&grant).execute(&owner).await.unwrap();
    outcome.unwrap_or_else(|panic| resume_unwind(panic))
}
