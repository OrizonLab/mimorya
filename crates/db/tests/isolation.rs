//! Tenant isolation through row-level security, with the application role.

mod support;

use mimorya_core::demo::{tenant, user};
use mimorya_db::Database;
use mimorya_db::repositories::tenants::{NewTenant, create_tenant};
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use uuid::Uuid;

async fn app() -> Database {
    support::connect(&support::urls(|_| {}).app).await
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn connects_with_a_role_that_cannot_bypass_rls() {
    let db = app().await;
    let (superuser, bypass): (bool, bool) =
        sqlx::query_as("select rolsuper, rolbypassrls from pg_roles where rolname = current_user")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(!superuser && !bypass);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn sees_nothing_without_a_tenant_context() {
    let db = app().await;
    let users: i64 = sqlx::query_scalar("select count(*) from users")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let memories: i64 = sqlx::query_scalar("select count(*) from memory_items")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!((users, memories), (0, 0));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn sees_only_the_rows_of_the_current_tenant() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let tenants: Vec<uuid::Uuid> = sqlx::query_scalar("select tenant_id from users")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(tenants.len(), 10);
    assert!(tenants.iter().all(|t| *t == tenant::ID));
    let leaked: i64 = sqlx::query_scalar(
        "select count(*) from memory_items where content like '%Tarif confidentiel%'",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(leaked, 0);
    drop(tx);

    let mut other = db.begin(OTHER_TENANT_ID).await.unwrap();
    let emails: Vec<String> = sqlx::query_scalar("select email::text from users")
        .fetch_all(&mut *other)
        .await
        .unwrap();
    assert_eq!(emails, vec!["contact@autre-entreprise.example".to_string()]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn ignores_explicit_filters_targeting_another_tenant() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let rows: i64 = sqlx::query_scalar("select count(*) from memory_items where tenant_id = $1")
        .bind(OTHER_TENANT_ID)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn rejects_writes_into_another_tenant() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let insert = sqlx::query(
        "insert into memory_items (tenant_id, scope, kind, title, content, source) \
         values ($1, 'tenant', 'fact', 'Injection', 'tentative', $2)",
    )
    .bind(OTHER_TENANT_ID)
    .bind(json!({ "type": "manual" }))
    .execute(&mut *tx)
    .await;
    assert!(support::db_error(insert).contains("row-level security"));
    drop(tx);

    let mut tx = db.begin(tenant::ID).await.unwrap();
    let update = sqlx::query("update users set tenant_id = $1 where id = $2")
        .bind(OTHER_TENANT_ID)
        .bind(user::LUCAS)
        .execute(&mut *tx)
        .await;
    assert!(support::db_error(update).contains("row-level security"));
}

/// The application role creates companies (ADR-033): only the one its
/// transaction is opened for.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn writes_no_company_but_the_transaction_s_own() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let foreign = create_tenant(
        &mut tx,
        NewTenant {
            id: Uuid::new_v4(),
            slug: "entreprise-forgee",
            name: "Entreprise forgée",
        },
    )
    .await;
    assert!(support::db_error(foreign).contains("row-level security"));
    drop(tx);

    let mut tx = db.begin(tenant::ID).await.unwrap();
    let renamed = sqlx::query("update tenants set name = 'Reprise' where id = $1")
        .bind(OTHER_TENANT_ID)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert_eq!(renamed.rows_affected(), 0);
    drop(tx);

    // Outside any company's transaction, no company can be written either.
    let mut conn = db.pool().acquire().await.unwrap();
    let id = Uuid::new_v4();
    let unscoped = create_tenant(
        &mut conn,
        NewTenant {
            id,
            slug: "entreprise-sans-contexte",
            name: "Entreprise sans contexte",
        },
    )
    .await;
    assert!(support::db_error(unscoped).contains("row-level security"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn keeps_the_audit_log_append_only() {
    let db = app().await;
    let mut tx = db.begin(tenant::ID).await.unwrap();
    sqlx::query(
        "insert into audit_log (tenant_id, actor_type, action, outcome) values ($1, 'system', 'test.append', 'success')",
    )
    .bind(tenant::ID)
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = db.begin(tenant::ID).await.unwrap();
    assert!(
        support::db_error(sqlx::query("delete from audit_log").execute(&mut *tx).await)
            .contains("permission denied")
    );
    drop(tx);
    let mut tx = db.begin(tenant::ID).await.unwrap();
    let update = sqlx::query("update audit_log set action = 'tampered'")
        .execute(&mut *tx)
        .await;
    assert!(support::db_error(update).contains("permission denied"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn cannot_erase_audit_entries_through_their_references() {
    let db = app().await;
    for table in ["tenants", "users", "conversations", "agent_runs"] {
        let mut tx = db.begin(tenant::ID).await.unwrap();
        let delete = sqlx::query(&format!("delete from {table}"))
            .execute(&mut *tx)
            .await;
        assert!(
            support::db_error(delete).contains("permission denied"),
            "{table}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn protects_every_tenant_scoped_table_with_rls_and_a_policy() {
    let db = app().await;
    let rows: Vec<(String, bool, i64)> = sqlx::query_as(
        "select c.relname::text, c.relrowsecurity, \
         (select count(*) from pg_policies p where p.tablename = c.relname) \
         from pg_class c join pg_namespace n on n.oid = c.relnamespace \
         where n.nspname = 'public' and c.relkind = 'r' and c.relname <> '_sqlx_migrations'",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert!(rows.len() >= 24);
    let unprotected: Vec<&String> = rows
        .iter()
        .filter(|(_, rls, policies)| !rls || *policies < 1)
        .map(|(name, ..)| name)
        .collect();
    assert!(
        unprotected.is_empty(),
        "tables without RLS: {unprotected:?}"
    );

    // The migration journal is the owner's only.
    let journal = sqlx::query("select 1 from _sqlx_migrations limit 1")
        .fetch_optional(db.pool())
        .await;
    assert!(support::db_error(journal).contains("permission denied"));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn exposes_only_the_minimal_pre_tenant_lookups() {
    let db = app().await;
    let login: Vec<(uuid::Uuid, uuid::Uuid)> =
        sqlx::query_as("select user_id, tenant_id from auth_find_login($1)")
            .bind("MARC.LEFEVRE@neo-maintenance.example")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(login, vec![(user::MARC, tenant::ID)]);
    let sessions = mimorya_db::repositories::auth::resolve_session(db.pool(), "unknown")
        .await
        .unwrap();
    assert!(sessions.is_none());
}
