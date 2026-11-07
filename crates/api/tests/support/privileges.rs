//! A privilege of the application role taken away for one step, as a
//! failing database would, and given back as it was whatever the step
//! does: the tests that follow in the same binary share the database. A
//! table the role may insert into only some columns of (the audit log,
//! whose seal it never writes, ADR-178) gets back only those.

use std::panic::{AssertUnwindSafe, resume_unwind};

use futures::FutureExt;

use super::owner_url;

/// Runs `step` while the application role may not insert into `table`.
pub async fn without_insert<T>(table: &str, step: impl Future<Output = T>) -> T {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let grant = insert_grant(&owner, table).await;
    let revoke = format!("revoke insert on {table} from mimorya_app");
    sqlx::query(&revoke).execute(&owner).await.unwrap();
    let outcome = AssertUnwindSafe(step).catch_unwind().await;
    sqlx::query(&grant).execute(&owner).await.unwrap();
    owner.close().await;
    outcome.unwrap_or_else(|panic| resume_unwind(panic))
}

/// The statement that gives the application role back what it may insert
/// into `table` now: the whole table, or only some columns.
async fn insert_grant(owner: &sqlx::PgPool, table: &str) -> String {
    let whole: bool = sqlx::query_scalar("select has_table_privilege('mimorya_app', $1, 'INSERT')")
        .bind(table)
        .fetch_one(owner)
        .await
        .unwrap();
    if whole {
        return format!("grant insert on {table} to mimorya_app");
    }
    let columns: Vec<String> = sqlx::query_scalar(
        "select column_name::text from information_schema.column_privileges \
         where table_schema = 'public' and table_name = $1 and grantee = 'mimorya_app' \
         and privilege_type = 'INSERT' order by column_name",
    )
    .bind(table)
    .fetch_all(owner)
    .await
    .unwrap();
    assert!(
        !columns.is_empty(),
        "the application role inserts nothing into {table}"
    );
    format!(
        "grant insert ({}) on {table} to mimorya_app",
        columns.join(", ")
    )
}
