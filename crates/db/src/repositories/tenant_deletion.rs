//! A company deleted whole by the operator (ADR-192), as the database's
//! owner: the application's role may not delete a company (no `DELETE` on
//! `tenants`), and no process of the application ever does. Each table of a
//! company names it (`tenant_id`) and follows its deletion (`ON DELETE
//! CASCADE`, `crates/db/tests/tenant_deletion.rs`): one statement deletes
//! all of it, in the caller's transaction. Row-level security does not bound
//! the owner: every statement here names the company itself.

use sqlx::PgConnection;
use uuid::Uuid;

/// The company whose slug is `slug`, its row locked until the end of the
/// transaction: its identifier and status.
pub async fn lock_tenant_by_slug(
    conn: &mut PgConnection,
    slug: &str,
) -> sqlx::Result<Option<(Uuid, String)>> {
    sqlx::query_as("select id, status from tenants where slug = $1 for update")
        .bind(slug)
        .fetch_optional(conn)
        .await
}

/// The company's runs still queued or under way.
pub async fn living_runs(conn: &mut PgConnection, tenant_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "select count(*) from agent_runs where tenant_id = $1 and status in ('queued', 'running')",
    )
    .bind(tenant_id)
    .fetch_one(conn)
    .await
}

/// The tables that name a company, as the catalog lists them.
async fn tenant_tables(conn: &mut PgConnection) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "select c.relname::text from pg_class c \
         join pg_namespace n on n.oid = c.relnamespace \
         join pg_attribute a on a.attrelid = c.oid and a.attname = 'tenant_id' \
           and not a.attisdropped \
         where n.nspname = 'public' and c.relkind in ('r', 'p') order by 1",
    )
    .fetch_all(conn)
    .await
}

/// How many rows each table holds of the company, the empty ones left out.
pub async fn tenant_rows(
    conn: &mut PgConnection,
    tenant_id: Uuid,
) -> sqlx::Result<Vec<(String, i64)>> {
    let mut rows = Vec::new();
    for table in tenant_tables(conn).await? {
        // Read from the catalog; quoted all the same.
        let quoted = table.replace('"', "\"\"");
        let count: i64 = sqlx::query_scalar(&format!(
            "select count(*) from \"{quoted}\" where tenant_id = $1"
        ))
        .bind(tenant_id)
        .fetch_one(&mut *conn)
        .await?;
        if count > 0 {
            rows.push((table, count));
        }
    }
    Ok(rows)
}

/// Deletes the company, and with it every row of every table that names
/// it; whether it was there.
pub async fn delete_tenant(conn: &mut PgConnection, tenant_id: Uuid) -> sqlx::Result<bool> {
    let deleted = sqlx::query("delete from tenants where id = $1")
        .bind(tenant_id)
        .execute(conn)
        .await?;
    Ok(deleted.rows_affected() == 1)
}
