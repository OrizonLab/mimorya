//! The run an audit entry names (ADR-186). The entry is checked when it is
//! written: the run exists, in the entry's company. Nothing names another
//! company's run nor a run that never was, whether the application writes
//! it, under row-level security, or the database's owner, past it. A run
//! deleted later, with its conversation, leaves the entry as it was
//! written, and so its seal (ADR-178).

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::testing::{OTHER_TENANT_ID, OTHER_USER_ID};
use sqlx::PgConnection;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn owner() -> Database {
    support::connect(&support::urls(|_| {}).owner).await
}

/// A conversation of `person` in `company` and a finished run in it.
async fn run(conn: &mut PgConnection, company: Uuid, person: Uuid) -> (Uuid, Uuid) {
    let conversation: Uuid = sqlx::query_scalar(
        "insert into conversations (tenant_id, owner_user_id, title) \
         values ($1, $2, 'Run nommé au journal') returning id",
    )
    .bind(company)
    .bind(person)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    let run = sqlx::query_scalar(
        "insert into agent_runs (tenant_id, conversation_id, on_behalf_of_user_id, objective, \
         budget, status) values ($1, $2, $3, 'objectif', '{}', 'completed') returning id",
    )
    .bind(company)
    .bind(conversation)
    .bind(person)
    .fetch_one(&mut *conn)
    .await
    .unwrap();
    (conversation, run)
}

/// An entry of `company` naming `run`, as the application writes one.
async fn entry(conn: &mut PgConnection, company: Uuid, run: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "insert into audit_log (tenant_id, actor_type, run_id, action, outcome) \
         values ($1, 'agent', $2, 'test.run_reference', 'success') returning id",
    )
    .bind(company)
    .bind(run)
    .fetch_one(conn)
    .await
}

/// Refused as a reference to no run of the entry's company.
fn refused(result: sqlx::Result<i64>) {
    let error = result.expect_err("the entry was written");
    let error = error.as_database_error().expect("a database error");
    assert_eq!(error.code().as_deref(), Some("23503"), "{error}");
    assert_eq!(error.constraint(), Some("audit_log_run_known"), "{error}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_entry_names_a_run_of_its_own_company_only() {
    // Another company's run, which does exist.
    let owner = owner().await;
    let mut conn = owner.pool().acquire().await.unwrap();
    let (other_conversation, other_run) = run(&mut conn, OTHER_TENANT_ID, OTHER_USER_ID).await;
    let (own_conversation, own_run) = run(&mut conn, tenant::ID, user::MARC).await;

    // The application, in its company's transaction: its own run alone.
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    refused(entry(&mut tx, tenant::ID, other_run).await);
    drop(tx);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    refused(entry(&mut tx, tenant::ID, Uuid::new_v4()).await);
    drop(tx);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    entry(&mut tx, tenant::ID, own_run).await.unwrap();
    drop(tx);

    // The owner, who sees every company's runs: the company is compared,
    // on an entry written as on one changed.
    let mut tx = owner.pool().begin().await.unwrap();
    refused(entry(&mut tx, tenant::ID, other_run).await);
    drop(tx);
    let mut tx = owner.pool().begin().await.unwrap();
    let id = entry(&mut tx, tenant::ID, own_run).await.unwrap();
    refused(
        sqlx::query_scalar::<_, i64>("update audit_log set run_id = $2 where id = $1 returning id")
            .bind(id)
            .bind(other_run)
            .fetch_one(&mut *tx)
            .await,
    );
    drop(tx);

    // Nothing of this test stays for the others.
    for conversation in [other_conversation, own_conversation] {
        sqlx::query("delete from conversations where id = $1")
            .bind(conversation)
            .execute(&mut *conn)
            .await
            .unwrap();
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_deleted_leaves_its_entries_as_written() {
    let owner = owner().await;
    let mut tx = owner.pool().begin().await.unwrap();
    let (conversation, run) = run(&mut tx, tenant::ID, user::MARC).await;
    let id = entry(&mut tx, tenant::ID, run).await.unwrap();

    sqlx::query("delete from conversations where id = $1")
        .bind(conversation)
        .execute(&mut *tx)
        .await
        .unwrap();
    let runs: i64 = sqlx::query_scalar("select count(*) from agent_runs where id = $1")
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(runs, 0);
    let (company, named): (Uuid, Option<Uuid>) =
        sqlx::query_as("select tenant_id, run_id from audit_log where id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!((company, named), (tenant::ID, Some(run)));
    // Nothing of this test stays for the others.
    tx.rollback().await.unwrap();
}
