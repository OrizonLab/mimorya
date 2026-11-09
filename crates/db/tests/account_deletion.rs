//! An account deleted by the application (ADR-189): only when erasing it,
//! so only a disabled account, and only of the company its transaction is
//! opened for, whatever the query asks. Deleting it takes every row naming
//! it with it, or empties the column: no key naming an account stops the
//! deletion.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::testing::OTHER_TENANT_ID;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn owner() -> Database {
    support::connect(&support::urls(|_| {}).owner).await
}

/// Rows the application deletes from `users` with `query`, `bound`.
async fn deleted(conn: &mut sqlx::PgConnection, query: &str, bound: Uuid) -> u64 {
    sqlx::query(query)
        .bind(bound)
        .execute(conn)
        .await
        .unwrap()
        .rows_affected()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_application_deletes_a_disabled_account_of_its_company_alone() {
    let db = app().await;
    let owner = owner().await;

    // Lucas, active: kept, though of the company.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let by_id = "delete from users where id = $1";
    assert_eq!(deleted(&mut tx, by_id, user::LUCAS).await, 0);
    drop(tx);

    // A disabled account of another company: neither by its identifier,
    // nor by its company, nor outside any company.
    let stranger = Uuid::new_v4();
    sqlx::query(
        "insert into users (id, tenant_id, email, display_name, status) \
         values ($1, $2, 'compte.desactive@autre.example', 'Compte désactivé', 'disabled')",
    )
    .bind(stranger)
    .bind(OTHER_TENANT_ID)
    .execute(owner.pool())
    .await
    .unwrap();
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let from_the_company = [
        deleted(&mut tx, by_id, stranger).await,
        deleted(
            &mut tx,
            "delete from users where tenant_id = $1",
            OTHER_TENANT_ID,
        )
        .await,
    ];
    drop(tx);
    let mut outside = db.pool().acquire().await.unwrap();
    let from_outside = deleted(&mut outside, by_id, stranger).await;
    drop(outside);
    // Removed before asserting: nothing of this test stays for the others.
    let still_there = sqlx::query("delete from users where id = $1")
        .bind(stranger)
        .execute(owner.pool())
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(from_the_company, [0, 0]);
    assert_eq!(from_outside, 0);
    assert_eq!(still_there, 1);

    // Lucas, disabled: deleted, in a transaction rolled back.
    let mut tx = db.begin(tenant::ID).await.unwrap();
    sqlx::query("update users set status = 'disabled' where id = $1")
        .bind(user::LUCAS)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(deleted(&mut tx, by_id, user::LUCAS).await, 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_key_naming_an_account_deletes_its_rows_or_empties_its_column() {
    let db = app().await;
    let (keys, stopping): (i64, Vec<String>) = sqlx::query_as(
        "select count(*), \
         coalesce(array_agg(conrelid::regclass::text || '.' || conname::text order by conname) \
           filter (where confdeltype not in ('c', 'n')), '{}') \
         from pg_constraint \
         where contype = 'f' and confrelid = 'users'::regclass",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert!(
        stopping.is_empty(),
        "keys stopping an erasure: {stopping:?}"
    );
    // The check looks at the schema's keys, not at none of them.
    assert!(keys >= 50, "{keys} keys name an account");
}
