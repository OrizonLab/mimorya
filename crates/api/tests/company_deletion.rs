//! A suspended company deleted by the operator (`mimorya-admin
//! delete-company`, ADR-192), as the database's owner. The demo company,
//! its people having something of every kind and its audit log sealed, is
//! shown first, table by table, with what gives access at other services,
//! and nothing goes; confirmed, it is deleted whole: no table keeps a row
//! of it, its sessions and sign-ins are gone, and every process is told to
//! let go of what it kept of it. Another company keeps every row it had.

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::audit_chain::SealedHeads;
use mimorya_api::services::provisioning::{CompanyDeletion, CompanyStatusChange, delete_company};
use mimorya_core::demo::{tenant, user};
use mimorya_db::notifications::listen_invalidations;
use support::audit_chain::seal;
use support::companies::{create, rows_of};
use support::personal_belongings::{belongings, company_parts};
use support::retention::owner;
use support::suspension::change;
use support::{PASSWORD, TestApi, app_url, email};
use uuid::Uuid;

/// What the invalidations heard until `count` of them asked to let go of.
async fn released(listener: &mut sqlx::postgres::PgListener, count: usize) -> BTreeSet<String> {
    let mut released = BTreeSet::new();
    while released.len() < count {
        let notification = tokio::time::timeout(Duration::from_secs(10), listener.recv())
            .await
            .expect("an announcement")
            .unwrap();
        let announced: serde_json::Value = serde_json::from_str(notification.payload()).unwrap();
        released.insert(format!("{}:{}", announced["kind"], announced["id"]).replace('"', ""));
    }
    released
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_is_deleted_whole_and_another_keeps_every_row() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let neo = company_parts(tenant::ID, "neo", 2001).await;
    belongings(&neo, user::MARC, "de-marc", None, 41, 10).await;
    belongings(&neo, user::LUCAS, "de-lucas", None, 42, 10).await;
    let sealed = seal(&api.app.services.db, &SealedHeads::default(), tenant::ID).await;
    assert!(sealed.unwrap() > 0);
    let other = create(&api, "acme-restante", "jeanne.martin@acme-restante.example").await;
    let owner = owner().await;
    let others = rows_of(&owner, other.tenant_id).await;
    assert!(!others.is_empty());

    change(&api, tenant::SLUG, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    let CompanyDeletion::Shown(shown) = delete_company(&owner, tenant::SLUG, None).await.unwrap()
    else {
        panic!("deleted without its slug confirmed");
    };
    assert_eq!(shown.tenant_id, tenant::ID);
    let tables: Vec<&str> = shown
        .tables
        .iter()
        .map(|(table, _)| table.as_str())
        .collect();
    for table in [
        "agent_runs",
        "audit_log",
        "connector_instances",
        "conversations",
        "memory_items",
        "sessions",
        "users",
    ] {
        assert!(tables.contains(&table), "{table} not in {tables:?}");
    }
    assert_eq!(rows_of(&owner, tenant::ID).await, shown.tables);
    // What gives access at other services is counted, to withdraw there.
    let outside = shown.held_outside();
    for what in [
        "autorisations OAuth de serveurs MCP données par ses personnes",
        "comptes Microsoft 365 ou Google Workspace reliés par ses personnes",
        "passerelles de conversation (Slack, Telegram, Teams)",
    ] {
        assert!(
            outside
                .iter()
                .any(|(said, count)| *said == what && *count > 0),
            "{what} not in {outside:?}"
        );
    }

    let connectors: Vec<Uuid> =
        sqlx::query_scalar("select id from connector_instances where tenant_id = $1")
            .bind(tenant::ID)
            .fetch_all(&owner)
            .await
            .unwrap();
    let mut listener = listen_invalidations(app_url()).await.unwrap();
    let confirmed = delete_company(&owner, &format!(" {} ", tenant::SLUG), Some(tenant::SLUG))
        .await
        .unwrap();
    let CompanyDeletion::Deleted(deleted) = confirmed else {
        panic!("not deleted once confirmed");
    };
    assert_eq!(deleted.tables, shown.tables);
    assert!(deleted.total() > 100, "{}", deleted.total());

    // Nothing of it is left, nobody of it is let in.
    assert_eq!(rows_of(&owner, tenant::ID).await, Vec::new());
    let companies: i64 = sqlx::query_scalar("select count(*) from tenants where id = $1")
        .bind(tenant::ID)
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(companies, 0);
    assert_eq!(marc.get("/auth/me").await.status, StatusCode::UNAUTHORIZED);
    let signing_in = api.try_login(&email(user::MARC), PASSWORD).await;
    assert_eq!(signing_in.status, StatusCode::UNAUTHORIZED);

    // Every process lets go of what it kept of it.
    let mut expected: BTreeSet<String> = ["access", "llm", "embeddings"]
        .iter()
        .map(|kind| format!("{kind}:{}", tenant::ID))
        .collect();
    expected.extend(connectors.iter().map(|id| format!("connector:{id}")));
    assert_eq!(released(&mut listener, expected.len()).await, expected);

    // Another company keeps every row it had.
    assert_eq!(rows_of(&owner, other.tenant_id).await, others);
    owner.close().await;
}
