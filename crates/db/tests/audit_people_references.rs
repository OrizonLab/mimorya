//! The people an audit entry names (ADR-189). The entry is checked when it
//! is written: the account acting, and the one it acts for, exist in the
//! entry's company. Nothing names another company's account nor one that
//! never was, whether the application writes it, under row-level security,
//! or the database's owner, past it. An account erased later leaves the
//! entry as it was written, and so its seal (ADR-178).

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::testing::OTHER_USER_ID;
use sqlx::PgConnection;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

async fn owner() -> Database {
    support::connect(&support::urls(|_| {}).owner).await
}

/// The column of an entry that names a person.
#[derive(Clone, Copy)]
enum Named {
    Actor,
    OnBehalfOf,
}

impl Named {
    fn column(self) -> &'static str {
        match self {
            Self::Actor => "actor_user_id",
            Self::OnBehalfOf => "on_behalf_of_user_id",
        }
    }
}

/// An entry of Néo Maintenance naming `person` in `named`, as the
/// application writes one.
async fn entry(conn: &mut PgConnection, named: Named, person: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(&format!(
        "insert into audit_log (tenant_id, actor_type, {}, action, outcome) \
         values ($1, 'user', $2, 'test.person_reference', 'success') returning id",
        named.column()
    ))
    .bind(tenant::ID)
    .bind(person)
    .fetch_one(conn)
    .await
}

/// Refused as a reference to no account of the entry's company.
fn refused(result: sqlx::Result<i64>) {
    let error = result.expect_err("the entry was written");
    let error = error.as_database_error().expect("a database error");
    assert_eq!(error.code().as_deref(), Some("23503"), "{error}");
    assert_eq!(
        error.constraint(),
        Some("audit_log_people_known"),
        "{error}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_entry_names_people_of_its_own_company_only() {
    let db = app().await;
    let owner = owner().await;
    for named in [Named::Actor, Named::OnBehalfOf] {
        // The application, in its company's transaction: its own people alone.
        for stranger in [OTHER_USER_ID, Uuid::new_v4()] {
            let mut tx = db.begin(tenant::ID).await.unwrap();
            refused(entry(&mut tx, named, stranger).await);
        }
        let mut tx = db.begin(tenant::ID).await.unwrap();
        entry(&mut tx, named, user::MARC).await.unwrap();
        drop(tx);

        // The owner, who sees every company's accounts: the company is
        // compared, on an entry written as on one changed.
        let mut tx = owner.pool().begin().await.unwrap();
        refused(entry(&mut tx, named, OTHER_USER_ID).await);
        drop(tx);
        let mut tx = owner.pool().begin().await.unwrap();
        let id = entry(&mut tx, named, user::MARC).await.unwrap();
        refused(
            sqlx::query_scalar::<_, i64>(&format!(
                "update audit_log set {} = $2 where id = $1 returning id",
                named.column()
            ))
            .bind(id)
            .bind(OTHER_USER_ID)
            .fetch_one(&mut *tx)
            .await,
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_account_deleted_leaves_its_entries_as_written() {
    let owner = owner().await;
    let mut tx = owner.pool().begin().await.unwrap();
    let person = Uuid::new_v4();
    sqlx::query(
        "insert into users (id, tenant_id, email, display_name) \
         values ($1, $2, 'compte.efface@neo-maintenance.example', 'Compte effacé')",
    )
    .bind(person)
    .bind(tenant::ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    let acting = entry(&mut tx, Named::Actor, person).await.unwrap();
    let acted_for = entry(&mut tx, Named::OnBehalfOf, person).await.unwrap();

    sqlx::query("delete from users where id = $1")
        .bind(person)
        .execute(&mut *tx)
        .await
        .unwrap();
    let named: Vec<(Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
        "select actor_user_id, on_behalf_of_user_id from audit_log \
         where id = any($1) order by id",
    )
    .bind([acting, acted_for])
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    assert_eq!(named, [(Some(person), None), (None, Some(person))]);
    // Nothing of this test stays for the others.
    tx.rollback().await.unwrap();
}
