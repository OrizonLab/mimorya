//! The caches every process keeps, shared (ADR-085): what an `api` process
//! changes in a company's policies or embedding model reaches a worker at
//! once, well within the lifetime of what the worker had loaded; a memory
//! written through an `api` process is indexed by a worker at once; why
//! the indexing a worker runs fails is shown through an `api` process,
//! until it succeeds or nothing is left to index; a connector a test
//! through an `api` process found answering is tried again by a worker at
//! once, not a minute later.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::embeddings::EmbeddingResolution;
use mimorya_connectors::testing::{Quirk, serve_mcp_http_on};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::TestApi;
use support::embeddings::{
    MODEL, VANS, back_to_words, choose, forget, index, remember, start_as, start_indexing_as,
    vector_of,
};
use support::mcp::{PRIVATE_DEPLOYMENT, add, mcp_body};
use support::queue::{API, WORKER, listening};
use uuid::Uuid;

/// Well within the 30 s a company's access is kept, a minute its model.
const AT_ONCE: usize = 50;
const STEP: Duration = Duration::from_millis(100);

fn identity(user_id: Uuid) -> AuthIdentity {
    AuthIdentity {
        tenant_id: tenant::ID,
        user_id,
        session_id: Uuid::nil(),
        must_change_password: false,
    }
}

async fn may_apply(worker: &TestApi, user_id: Uuid) -> bool {
    let access = &worker.app.services.access;
    access
        .context(identity(user_id))
        .await
        .unwrap()
        .has("planning:apply")
}

/// Whether the worker sees `expected` within `AT_ONCE` steps.
async fn applies_at_once(worker: &TestApi, user_id: Uuid, expected: bool) -> bool {
    for _ in 0..AT_ONCE {
        if may_apply(worker, user_id).await == expected {
            return true;
        }
        tokio::time::sleep(STEP).await;
    }
    false
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_policy_written_through_an_api_process_applies_in_a_worker_at_once() {
    let api = TestApi::start_with(API).await;
    let worker = TestApi::start_with(WORKER).await;
    listening(&worker).await;
    // Loaded by the worker, and kept.
    assert!(may_apply(&worker, user::MARC).await);

    let paul = api.sign_in(user::PAUL).await;
    let people: Value = json!({
        "op": "eq",
        "left": { "attr": "context.actorType" },
        "right": { "value": "user" },
    });
    let body = json!({
        "description": "Personne n’applique de planning",
        "permissions": ["planning:apply"],
        "condition": people,
    });
    let created = paul.post("/admin/policies", body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_string();
    assert!(applies_at_once(&worker, user::MARC, false).await);

    let deleted = paul.delete(&format!("/admin/policies/{id}")).await;
    assert_eq!(deleted.status, StatusCode::OK, "{}", deleted.text());
    assert!(applies_at_once(&worker, user::MARC, true).await);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_worker_indexes_with_the_model_chosen_through_an_api_process_and_its_failure_is_shown() {
    let api = start_as("api").await;
    let worker = start_as("worker").await;
    listening(&worker).await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    // The worker read the search by words, and keeps it.
    let resolution = worker.app.services.embeddings.resolve(tenant::ID).await;
    assert!(matches!(
        resolution.unwrap().as_ref(),
        EmbeddingResolution::Off
    ));

    choose(&paul, MODEL, 0.15).await;
    let mut indexed = 0;
    for _ in 0..AT_ONCE {
        indexed = index(&worker).await;
        if indexed > 0 {
            break;
        }
        tokio::time::sleep(STEP).await;
    }
    assert!(indexed > 0);

    // Failing: recorded by the worker, shown through the api process.
    worker.embedder.fail(true);
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    assert_eq!(index(&worker).await, 0);
    assert_eq!(shown_failure(&paul).await, "Fournisseur indisponible");
    // Nothing left to index: no failure left to show.
    forget(&lucas, vans).await;
    assert_eq!(shown_failure(&paul).await, Value::Null);
    let vans = remember(&lucas, VANS.0, VANS.1).await;
    assert_eq!(index(&worker).await, 0);
    assert_eq!(shown_failure(&paul).await, "Fournisseur indisponible");
    // Back up: forgotten at the next pass.
    worker.embedder.fail(false);
    assert_eq!(index(&worker).await, 1);
    assert_eq!(shown_failure(&paul).await, Value::Null);

    forget(&lucas, vans).await;
    back_to_words(&paul).await;
}

async fn shown_failure(admin: &support::Client<'_>) -> Value {
    let shown = admin.get("/admin/embedding").await.json();
    shown["settings"]["indexingError"].clone()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_memory_written_through_an_api_process_is_indexed_by_a_worker_at_once() {
    let api = start_as("api").await;
    let worker = start_indexing_as("worker").await;
    listening(&worker).await;
    let paul = api.sign_in(user::PAUL).await;
    let lucas = api.sign_in(user::LUCAS).await;
    choose(&paul, MODEL, 0.15).await;
    // The memory indexed: the worker waits for a minute, or a wake.
    let mut progress = Value::Null;
    for _ in 0..AT_ONCE {
        progress = paul.get("/admin/embedding").await.json()["settings"].clone();
        if progress["embedded"] == progress["memories"] {
            break;
        }
        tokio::time::sleep(STEP).await;
    }
    assert_eq!(progress["embedded"], progress["memories"], "{progress}");

    let vans = remember(&lucas, VANS.0, VANS.1).await;
    let mut indexed = vector_of(&api, vans).await;
    for _ in 0..AT_ONCE {
        if indexed.1 {
            break;
        }
        tokio::time::sleep(STEP).await;
        indexed = vector_of(&api, vans).await;
    }
    assert_eq!(indexed, (Some(MODEL.into()), true));
    forget(&lucas, vans).await;
    back_to_words(&paul).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_connector_a_test_through_an_api_process_found_answering_is_tried_again_by_a_worker() {
    let api = TestApi::start_with(&[PRIVATE_DEPLOYMENT[0], PRIVATE_DEPLOYMENT[1], API[0]]).await;
    let worker =
        TestApi::start_with(&[PRIVATE_DEPLOYMENT[0], PRIVATE_DEPLOYMENT[1], WORKER[0]]).await;
    listening(&worker).await;
    let paul = api.sign_in(user::PAUL).await;
    // Nothing listens on this port yet.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}/mcp");
    let id = add(&paul, mcp_body("Procédures", "docs", &url, None)).await;
    let connectors = &worker.app.services.connectors;
    let issues = || async { connectors.tools_for(tenant::ID).await.unwrap().issues.len() };
    // The worker remembers the failure for a minute.
    assert_eq!(issues().await, 1);

    serve_mcp_http_on(port, None, Quirk::None).await;
    assert_eq!(issues().await, 1);
    let tested = paul
        .post(&format!("/admin/connectors/{id}/test"), json!({}))
        .await;
    assert_eq!(tested.json()["ok"], true);
    let mut left = 1;
    for _ in 0..AT_ONCE {
        left = issues().await;
        if left == 0 {
            break;
        }
        tokio::time::sleep(STEP).await;
    }
    assert_eq!(left, 0);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_worker_whose_listener_was_cut_forgets_what_it_kept() {
    let worker = TestApi::start_with(WORKER).await;
    listening(&worker).await;
    assert!(may_apply(&worker, user::MARC).await);
    // Written where no process announces it: as if its notice were lost.
    let policy = as_owner(
        "insert into access_policies (tenant_id, description, effect, permissions, condition) \
         values ($1, 'Annonce perdue', 'deny', array['planning:apply'], \
         '{\"op\":\"eq\",\"left\":{\"attr\":\"context.actorType\"},\"right\":{\"value\":\"user\"}}') \
         returning id",
    )
    .await;
    assert!(may_apply(&worker, user::MARC).await);

    let app = sqlx::PgPool::connect(support::app_url()).await.unwrap();
    sqlx::query(
        "select pg_terminate_backend(pid) from pg_stat_activity \
         where application_name = 'mimorya-invalidations' and datname = current_database()",
    )
    .execute(&app)
    .await
    .unwrap();
    app.close().await;
    // Connected again, it forgot.
    assert!(applies_at_once(&worker, user::MARC, false).await);
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    sqlx::query("delete from access_policies where id = $1")
        .bind(policy)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

/// Runs `insert` as the owner, for the demonstration company; the row's id.
async fn as_owner(insert: &str) -> Uuid {
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    let id = sqlx::query_scalar(insert)
        .bind(tenant::ID)
        .fetch_one(&owner)
        .await
        .unwrap();
    owner.close().await;
    id
}
