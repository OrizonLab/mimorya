//! What a person's own renewal (ADR-174) meets while its token endpoint
//! answers: their withdrawal, after which the tokens it brings back, which
//! no one keeps, are revoked as theirs were (ADR-172); their authorizing
//! again with the same client at the same server, after which nothing is
//! revoked, their new authorization kept; a refusal of the old grant, which
//! removes nothing and leaves the session of their new authorization to
//! the run that holds it.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::connectors::{ConnectorService, Served};
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::connector_sessions::{as_person, sees_for};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, person_authorize, person_kept,
};
use support::person_renewals::{
    SOON, asked_at, for_person, person_refresh, person_removals, revoked_until, token_asked,
};
use support::sso::location;
use tokio::time::sleep;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_tokens_of_their_renewal_in_flight_when_withdrawn_are_revoked() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "envolsoi").await;
    let back = person_authorize(&marc, &authority, &id).await;
    assert_eq!(location(&back), PERSON_AUTHORIZED);
    let access = person_kept(&api, &id, user::MARC).await.expect("kept");
    let refresh = person_refresh(&api, &id, user::MARC)
        .await
        .expect("renewable");
    // His renewal issued at the server, its answer kept back meanwhile.
    for_person(&api, &id, user::MARC, SOON).await;
    authority.set(|settings| settings.held_renewals = Some(Duration::from_secs(2)));
    let path = format!("/account/connector-authorizations/{id}");
    let withdrawn = async {
        token_asked(&authority, 2).await;
        marc.delete(&path).await
    };
    let connectors = &api.app.services.connectors;
    let run = sees_for(connectors, "envolsoi", as_person(user::MARC));
    let (seen, withdrawn) = tokio::join!(run, withdrawn);
    assert!(!seen);
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    // His withdrawn tokens revoked, then the renewal's, which no one keeps.
    let revoked = revoked_until(&authority, 4).await;
    let tokens: Vec<_> = revoked.iter().map(|r| r.params["token"].as_str()).collect();
    assert_eq!(tokens.len(), 4, "{tokens:?}");
    assert_eq!(tokens[..2], [refresh.as_str(), access.as_str()]);
    let hints: Vec<_> = revoked[2..]
        .iter()
        .map(|r| r.params["token_type_hint"].as_str())
        .collect();
    assert_eq!(hints, ["refresh_token", "access_token"]);
    assert!(tokens[2..].iter().all(|t| *t != refresh && *t != access));
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn their_renewal_in_flight_when_they_authorize_again_revokes_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "relaissoi").await;
    let back = person_authorize(&marc, &authority, &id).await;
    assert_eq!(location(&back), PERSON_AUTHORIZED);
    for_person(&api, &id, user::MARC, SOON).await;
    authority.set(|settings| settings.held_renewals = Some(Duration::from_secs(2)));
    let again = async {
        token_asked(&authority, 2).await;
        person_authorize(&marc, &authority, &id).await
    };
    let connectors = &api.app.services.connectors;
    let run = sees_for(connectors, "relaissoi", as_person(user::MARC));
    let (_, again) = tokio::join!(run, again);
    assert_eq!(location(&again), PERSON_AUTHORIZED);
    // His new authorization kept; nothing revoked at its server, the task
    // that would revoke them given its time.
    sleep(Duration::from_secs(2)).await;
    assert!(asked_at(&authority, "revoke").is_empty());
    let access = person_kept(&api, &id, user::MARC).await.expect("kept");
    assert!(authority.accepts(&access, &server.url));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

/// The first of the server's tools given to `served`'s run, if any: the
/// very one its session holds.
async fn first_tool(
    connectors: &ConnectorService,
    served: Served,
    prefix: &str,
) -> Option<mimorya_core::tools::ToolRef> {
    let tools = connectors.tools_for(tenant::ID, served).await.unwrap();
    let namespace = format!("mcp_{prefix}_");
    tools
        .tools
        .into_iter()
        .find(|tool| tool.info().name.starts_with(&namespace))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_refusal_of_the_old_grant_leaves_the_session_of_their_new_one() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "refusanc").await;
    let back = person_authorize(&marc, &authority, &id).await;
    assert_eq!(location(&back), PERSON_AUTHORIZED);
    // His grant gone at the server, its refusal kept back meanwhile.
    for_person(&api, &id, user::MARC, SOON).await;
    authority.revoke_all();
    authority.set(|settings| settings.held_renewals = Some(Duration::from_secs(4)));
    let connectors = &api.app.services.connectors;
    let as_marc = as_person(user::MARC);
    // He authorizes again, and another run of his starts its session with
    // it, before the refusal arrives.
    let again = async {
        token_asked(&authority, 2).await;
        let again = person_authorize(&marc, &authority, &id).await;
        assert_eq!(location(&again), PERSON_AUTHORIZED);
        first_tool(connectors, as_marc, "refusanc").await
    };
    let renewing = first_tool(connectors, as_marc, "refusanc");
    let (renewing, other) = tokio::join!(renewing, again);
    // Nothing removed; his new session left to the runs that hold it: the
    // renewing run is given the very same, none started again.
    let (renewing, other) = (renewing.expect("given"), other.expect("given"));
    assert!(std::sync::Arc::ptr_eq(&renewing, &other));
    assert!(person_removals(&api, &id).await.is_empty());
    assert!(person_kept(&api, &id, user::MARC).await.is_some());
    assert_eq!(connectors.person_sessions().await, 1);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
