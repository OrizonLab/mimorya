//! What an erasure test reads (ADR-189), as an operator does: every row
//! naming a person, table by table, found from the keys naming an
//! account, and what the audit log keeps naming them.

use uuid::Uuid;

use super::retention::owner;

/// For each column whose key names an account (`table.column`), how many
/// rows name `person`.
pub async fn rows_naming(person: Uuid) -> Vec<(String, i64)> {
    let owner = owner().await;
    let columns: Vec<(String, String)> = sqlx::query_as(
        "select c.conrelid::regclass::text, a.attname::text from pg_constraint c \
         join pg_attribute a on a.attrelid = c.conrelid and a.attnum = any (c.conkey) \
         where c.contype = 'f' and c.confrelid = 'users'::regclass \
           and a.attname <> 'tenant_id' \
         order by 1, 2",
    )
    .fetch_all(&owner)
    .await
    .unwrap();
    let mut counts = Vec::new();
    for (table, column) in columns {
        let count: i64 =
            sqlx::query_scalar(&format!("select count(*) from {table} where {column} = $1"))
                .bind(person)
                .fetch_one(&owner)
                .await
                .unwrap();
        counts.push((format!("{table}.{column}"), count));
    }
    owner.close().await;
    counts
}

/// The audit entries naming `person`: acting, acted for, or the account
/// changed.
pub async fn entries_naming(person: Uuid) -> Vec<i64> {
    let owner = owner().await;
    let entries = sqlx::query_scalar(
        "select id from audit_log where actor_user_id = $1 or on_behalf_of_user_id = $1 \
         or (resource_type = 'user' and resource_id = $1::text) order by id",
    )
    .bind(person)
    .fetch_all(&owner)
    .await
    .unwrap();
    owner.close().await;
    entries
}

/// Whether the account `person` is still there.
pub async fn exists(person: Uuid) -> bool {
    let owner = owner().await;
    let found = sqlx::query_scalar("select exists (select 1 from users where id = $1)")
        .bind(person)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    found
}
