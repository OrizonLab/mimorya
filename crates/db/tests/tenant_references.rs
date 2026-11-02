//! References between company tables carry the company (ADR-167): a row
//! never points at a row of another company, not even through a write that
//! row-level security lets through, since PostgreSQL checks foreign keys
//! past it. The schema's every such key compares (tenant_id, id), and the
//! keys that set a reference to null on deletion leave the company alone.

mod support;

use mimorya_core::demo::tenant;
use mimorya_db::Database;
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::json;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn owner() -> Database {
    support::connect(&support::urls(|_| {}).owner).await
}

/// The keys from a company table to another one (but tenants) that do not
/// compare the company first, on both sides, or that would set it to null.
const LOOSE_KEYS: &str = "\
    with keys as ( \
      select c.conname::text as name, c.conkey, c.confkey, c.confdeltype, c.confdelsetcols, \
             (select attnum from pg_attribute where attrelid = c.conrelid and attname = 'tenant_id') as child, \
             (select attnum from pg_attribute where attrelid = c.confrelid and attname = 'tenant_id') as parent \
      from pg_constraint c \
      where c.contype = 'f' and c.connamespace = 'public'::regnamespace \
        and c.confrelid <> 'tenants'::regclass) \
    select name from keys \
    where child is not null and parent is not null \
      and (conkey[1] <> child or confkey[1] <> parent \
           or (confdeltype = 'n' and (confdelsetcols is null or child = any(confdelsetcols)))) \
    order by name";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_reference_between_company_tables_carries_the_company() {
    let db = app().await;
    let loose: Vec<String> = sqlx::query_scalar(LOOSE_KEYS)
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert!(loose.is_empty(), "keys without the company: {loose:?}");

    // The check looks at the schema's keys, not at none of them.
    let composite: i64 = sqlx::query_scalar(
        "select count(*) from pg_constraint c \
         where c.contype = 'f' and c.connamespace = 'public'::regnamespace \
           and c.confrelid <> 'tenants'::regclass \
           and c.conkey[1] = (select attnum from pg_attribute \
                              where attrelid = c.conrelid and attname = 'tenant_id')",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(composite >= 100, "{composite} keys compare the company");
}

/// Row-level security lets a company's transaction write its own rows; the
/// key refuses the one that names another company's user.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_row_cannot_point_at_another_company_s_row() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let insert = sqlx::query(
        "insert into memory_items (tenant_id, scope, kind, title, content, source, owner_user_id) \
         values ($1, 'user', 'fact', 'Référence croisée', 'tentative', $2, $3)",
    )
    .bind(tenant::ID)
    .bind(json!({ "type": "manual" }))
    .bind(OTHER_USER_ID)
    .execute(&mut *tx)
    .await;
    assert!(support::db_error(insert).contains("memory_items_owner_user_id_users_id_fk"));
}

/// A key that sets its reference to null on deletion names that column
/// alone: the row keeps its company, and the deletion goes through.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_deleted_reference_leaves_the_row_in_its_company() {
    let db = owner().await;
    let mut tx = db.pool().begin().await.unwrap();
    let user = Uuid::new_v4();
    sqlx::query(
        "insert into users (id, tenant_id, email, display_name) \
         values ($1, $2, 'reference.supprimee@neo-maintenance.example', 'Référence supprimée')",
    )
    .bind(user)
    .bind(tenant::ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    let entry: i64 = sqlx::query_scalar(
        "insert into audit_log (tenant_id, actor_type, actor_user_id, action, outcome) \
         values ($1, 'user', $2, 'test.reference', 'success') returning id",
    )
    .bind(tenant::ID)
    .bind(user)
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    sqlx::query("delete from users where id = $1")
        .bind(user)
        .execute(&mut *tx)
        .await
        .unwrap();
    let (company, actor): (Uuid, Option<Uuid>) =
        sqlx::query_as("select tenant_id, actor_user_id from audit_log where id = $1")
            .bind(entry)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!((company, actor), (tenant::ID, None));
    // Nothing of this test stays for the others.
    tx.rollback().await.unwrap();
}
