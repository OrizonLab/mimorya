//! A company suspended and reactivated by the operator (`mimorya-admin`,
//! ADR-191), named by its slug: suspended, its sessions are refused at once
//! and nobody of it signs in, while another company carries on; reactivated,
//! its sessions not yet expired and its sign-ins are accepted again. Each
//! change is audited by `system`, and every process is told to let go of
//! what it keeps of the company: a listening worker sees the change at
//! once, well before its cached access expires. An unknown slug, or a
//! company already in the status asked, changes nothing (one changed
//! meanwhile: `company_suspension_race.rs`).

mod support;

use std::collections::BTreeSet;
use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::provisioning::CompanyStatusChange;
use mimorya_api::services::runs::COMPANY_NOT_ACTIVE;
use mimorya_core::demo::{tenant, user};
use mimorya_core::{DomainError, ErrorCode};
use mimorya_db::notifications::listen_invalidations;
use serde_json::Value;
use support::TestApi;
use support::accounts::{INVITED_PASSWORD, accept};
use support::companies::create;
use support::queue::{WORKER, listening};
use support::suspension::{DemoActiveOnDrop, change, refused, state};
use uuid::Uuid;

const SLUG: &str = "acme-suspendue";
const ADMIN: &str = "jeanne.martin@acme-suspendue.example";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_is_closed_at_once_and_reopened_with_its_sessions() {
    let api = TestApi::start().await;
    let created = create(&api, SLUG, ADMIN).await;
    let accepted = accept(&api, &created.invitation.link, INVITED_PASSWORD).await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let jeanne = api.session_from(&accepted);
    let other = api.sign_in(user::MARC).await;
    assert_eq!(jeanne.get("/auth/me").await.status, StatusCode::OK);

    let suspended = change(&api, &format!(" {SLUG} "), CompanyStatusChange::Suspend)
        .await
        .unwrap();
    assert_eq!(suspended, created.tenant_id);
    assert_eq!(
        jeanne.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    let signing_in = api.try_login(ADMIN, INVITED_PASSWORD).await;
    assert_eq!(signing_in.status, StatusCode::UNAUTHORIZED);
    assert_eq!(other.get("/auth/me").await.status, StatusCode::OK);
    let (status, entries) = state(&api, created.tenant_id, "tenant.suspended").await;
    assert_eq!((status.as_str(), entries), ("suspended", 1));

    let reactivated = change(&api, SLUG, CompanyStatusChange::Reactivate)
        .await
        .unwrap();
    assert_eq!(reactivated, created.tenant_id);
    let me = jeanne.get("/auth/me").await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.text());
    let signing_in = api.try_login(ADMIN, INVITED_PASSWORD).await;
    assert_eq!(signing_in.status, StatusCode::OK, "{}", signing_in.text());
    let (status, entries) = state(&api, created.tenant_id, "tenant.reactivated").await;
    assert_eq!((status.as_str(), entries), ("active", 1));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_known_company_in_the_other_status_changes() {
    let api = TestApi::start().await;
    let created = create(
        &api,
        "acme-deux-fois",
        "jeanne.martin@acme-deux-fois.example",
    )
    .await;

    let unknown = refused(&api, "nulle-part", CompanyStatusChange::Suspend).await;
    assert_eq!(unknown.code, ErrorCode::NotFound);
    assert_eq!(unknown.message, "Aucune entreprise ne porte ce sigle");
    // Another test of this file may have reactivated it already.
    let (_, before) = state(&api, tenant::ID, "tenant.reactivated").await;
    let active = refused(&api, tenant::SLUG, CompanyStatusChange::Reactivate).await;
    assert_eq!(active.code, ErrorCode::Conflict);
    assert_eq!(active.message, "Cette entreprise n’est pas suspendue");
    let (status, entries) = state(&api, tenant::ID, "tenant.reactivated").await;
    assert_eq!((status.as_str(), entries), ("active", before));

    change(&api, "acme-deux-fois", CompanyStatusChange::Suspend)
        .await
        .unwrap();
    let again = refused(&api, "acme-deux-fois", CompanyStatusChange::Suspend).await;
    assert_eq!(again.code, ErrorCode::Conflict);
    assert_eq!(again.message, "Cette entreprise est déjà suspendue");
    let (status, entries) = state(&api, created.tenant_id, "tenant.suspended").await;
    assert_eq!((status.as_str(), entries), ("suspended", 1));
}

/// The invalidations heard on the channel until `count` of them arrived.
async fn heard(listener: &mut sqlx::postgres::PgListener, count: usize) -> Vec<Value> {
    let mut heard = Vec::new();
    while heard.len() < count {
        let notification = tokio::time::timeout(Duration::from_secs(10), listener.recv())
            .await
            .expect("an announcement")
            .unwrap();
        heard.push(serde_json::from_str(notification.payload()).unwrap());
    }
    heard
}

/// What `announced` asks to let go of, as `kind:id`, and the origins.
fn released(announced: &[Value]) -> (BTreeSet<String>, BTreeSet<String>) {
    let what = announced
        .iter()
        .map(|a| {
            format!(
                "{}:{}",
                a["kind"].as_str().unwrap(),
                a["id"].as_str().unwrap()
            )
        })
        .collect();
    let origins = announced
        .iter()
        .map(|a| a["origin"].as_str().unwrap().to_string())
        .collect();
    (what, origins)
}

/// The demo company's status as `process` holds it for Marc's access.
async fn status_held(process: &TestApi) -> String {
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id: user::MARC,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    let access = process.app.services.access.context(identity).await.unwrap();
    access.tenant.status.clone()
}

/// Whether `process` holds `expected` within five seconds, well before the
/// 30 s its cached access lasts.
async fn holds_at_once(process: &TestApi, expected: &str) -> bool {
    for _ in 0..50 {
        if status_held(process).await == expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_process_lets_go_of_what_it_keeps_of_the_company() {
    let api = TestApi::start().await;
    let worker = TestApi::start_with(WORKER).await;
    listening(&worker).await;
    let _active = DemoActiveOnDrop;
    let connectors: Vec<Uuid> = {
        let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
        sqlx::query_scalar("select id from connector_instances")
            .fetch_all(&mut *tx)
            .await
            .unwrap()
    };
    assert!(!connectors.is_empty(), "the demo company has connectors");
    let mut expected: BTreeSet<String> = ["access", "llm", "embeddings"]
        .iter()
        .map(|kind| format!("{kind}:{}", tenant::ID))
        .collect();
    expected.extend(connectors.iter().map(|id| format!("connector:{id}")));
    let mut listener = listen_invalidations(support::app_url()).await.unwrap();
    // Loaded by the worker, and kept.
    assert_eq!(status_held(&worker).await, "active");

    for (asked, held) in [
        (CompanyStatusChange::Suspend, "suspended"),
        (CompanyStatusChange::Reactivate, "active"),
    ] {
        change(&api, tenant::SLUG, asked).await.unwrap();
        let announced = heard(&mut listener, expected.len()).await;
        let (what, origins) = released(&announced);
        assert_eq!(what, expected, "{asked:?}");
        // From the command alone: no process takes it for its own.
        assert_eq!(origins.len(), 1, "{announced:?}");
        assert!(holds_at_once(&worker, held).await, "{asked:?}");
    }
    let (status, _) = state(&api, tenant::ID, "tenant.suspended").await;
    assert_eq!(status, "active");
}

/// Why a run for Marc would be refused, as a scheduled task or a hook asks.
async fn refusal_for_marc(api: &TestApi) -> String {
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id: user::MARC,
        session_id: Uuid::nil(),
        must_change_password: false,
    };
    let Err(refused) = api.app.services.runs.ready_to_run(identity).await else {
        panic!("a run would start");
    };
    refused
        .downcast::<DomainError>()
        .expect("domain error")
        .message
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_suspended_company_is_refused_before_its_model_is_loaded() {
    // No model at all: what refuses comes first.
    let api = TestApi::without_llm().await;
    // The change reaches the process as it reaches any other, by the
    // invalidation it hears: the refusal is read once it holds the status.
    listening(&api).await;
    let _active = DemoActiveOnDrop;
    change(&api, tenant::SLUG, CompanyStatusChange::Suspend)
        .await
        .unwrap();
    assert!(holds_at_once(&api, "suspended").await);
    assert_eq!(refusal_for_marc(&api).await, COMPANY_NOT_ACTIVE);

    change(&api, tenant::SLUG, CompanyStatusChange::Reactivate)
        .await
        .unwrap();
    assert!(holds_at_once(&api, "active").await);
    let unavailable = refusal_for_marc(&api).await;
    assert!(unavailable.starts_with("Aucun modèle"), "{unavailable}");
}
