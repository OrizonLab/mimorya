//! A company deleted by the operator (ADR-192) takes every row of it with
//! it: each table that names a company follows its deletion, by a key on
//! that very column (`ON DELETE CASCADE`), so that no table can keep a row
//! of a company gone. And the application's role may not delete a company,
//! even its own: only the database's owner does.

mod support;

use mimorya_core::demo::tenant;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_table_naming_a_company_follows_its_deletion() {
    let urls = support::urls(|_| {});
    let db = support::connect(&urls.owner).await;
    let naming: Vec<(String, bool)> = sqlx::query_as(
        "select c.relname::text, exists ( \
           select 1 from pg_constraint k \
            where k.conrelid = c.oid and k.contype = 'f' \
              and k.confrelid = 'tenants'::regclass and k.confdeltype = 'c' \
              and k.conkey = array[a.attnum]) \
         from pg_class c \
         join pg_namespace n on n.oid = c.relnamespace \
         join pg_attribute a on a.attrelid = c.oid and a.attname = 'tenant_id' \
           and not a.attisdropped \
         where n.nspname = 'public' and c.relkind in ('r', 'p') order by 1",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();

    let kept: Vec<&str> = naming
        .iter()
        .filter(|(_, follows)| !follows)
        .map(|(table, _)| table.as_str())
        .collect();
    assert!(
        kept.is_empty(),
        "tables not following their company: {kept:?}"
    );
    // The catalog was read, not an empty schema.
    assert!(naming.len() > 50, "{} tables", naming.len());
    assert!(naming.iter().any(|(table, _)| table == "audit_log"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_may_not_delete_a_company_even_its_own() {
    let urls = support::urls(|_| {});
    let app = support::connect(&urls.app).await;
    let mut tx = app.begin(tenant::ID).await.unwrap();
    let refused = support::db_error(
        sqlx::query("delete from tenants where id = $1")
            .bind(tenant::ID)
            .execute(&mut *tx)
            .await,
    );
    assert!(refused.contains("permission denied"), "{refused}");
    drop(tx);

    let owner = support::connect(&urls.owner).await;
    let still: i64 = sqlx::query_scalar("select count(*) from tenants where id = $1")
        .bind(tenant::ID)
        .fetch_one(owner.pool())
        .await
        .unwrap();
    assert_eq!(still, 1);
}
