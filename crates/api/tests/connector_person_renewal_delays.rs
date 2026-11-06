//! A run's renewals wait for one another nowhere (ADR-171, ADR-174): the
//! company's authorization and the person's own, both due, their token
//! endpoints mute, are renewed side by side before the run reads its
//! instances, the run delayed by one token endpoint's bound, not by both
//! in turn, and given both servers' tools with their current tokens.

mod support;

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::connector_oauth::{AUTHORIZED, authorize, declared};
use support::connector_sessions::{alter, as_person, token_endpoint};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::person_authorizations::{PERSON_AUTHORIZED, declared_per_person, person_authorize};
use support::person_renewals::{SOON, for_person};
use support::sso::location;
use support::token_endpoints::mute;

/// The company's access token expiring in three minutes and a half.
const COMPANY_SOON: &str = "update connector_authorizations set access_expires_at = \
                            now() + interval '210 seconds' where connector_id = $1";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_renews_the_company_s_and_its_person_s_side_by_side() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;

    // The company's: authorized by Paul, due, its token endpoint mute.
    let (_, company) = declared(&paul, &authority, "cotecomp").await;
    let authorized = authorize(&paul, &authority, &company).await;
    assert_eq!(location(&authorized), AUTHORIZED);
    let (company_endpoint, company_calls) = mute().await;
    alter(&api, &company, &token_endpoint(&company_endpoint)).await;
    alter(&api, &company, COMPANY_SOON).await;
    // Marc's own: likewise.
    let (_, own) = declared_per_person(&paul, &authority, "cotesoi").await;
    let authorized = person_authorize(&marc, &authority, &own).await;
    assert_eq!(location(&authorized), PERSON_AUTHORIZED);
    let (own_endpoint, own_calls) = mute().await;
    alter(&api, &own, &token_endpoint(&own_endpoint)).await;
    for_person(&api, &own, user::MARC, SOON).await;

    // His run: both asked at once, one bound waited, both servers given.
    let connectors = &api.app.services.connectors;
    let started = Instant::now();
    let tools = connectors
        .tools_for(tenant::ID, as_person(user::MARC))
        .await
        .unwrap();
    let took = started.elapsed();
    assert_eq!(company_calls.load(Ordering::SeqCst), 1);
    assert_eq!(own_calls.load(Ordering::SeqCst), 1);
    assert!(took > Duration::from_secs(5), "{took:?}");
    assert!(took < Duration::from_secs(15), "{took:?}");
    for prefix in ["cotecomp", "cotesoi"] {
        let namespace = format!("mcp_{prefix}_");
        assert!(
            tools
                .tools
                .iter()
                .any(|tool| tool.info().name.starts_with(&namespace)),
            "{prefix}"
        );
    }
    for connector in [&company, &own] {
        let deleted = paul.delete(&format!("/admin/connectors/{connector}")).await;
        assert_eq!(deleted.status, StatusCode::OK);
    }
}
