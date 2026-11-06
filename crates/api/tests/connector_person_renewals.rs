//! A person's own OAuth access token of an MCP server renewed (ADR-174), as
//! the company's is (ADR-171): one about to expire is renewed before their
//! run reads their instance, never another person's; their session is
//! started again with the new token, the rotated refresh token is kept, and
//! their account shows it renewed. Two processes renew it once. A server
//! out of reach leaves it, the failure shown to them until it is renewed.
//! One saying the grant is gone removes it, audited by the system with
//! whose it was, never a token, and releases their session.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_api::services::connectors::{ConnectorService, Served, TO_AUTHORIZE};
use mimorya_connectors::testing::{CLIENT_SECRET, FakeAuthorizationServer, ProtectedMcp};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::connector_sessions::{alter, another_process, as_person, sees_for, token_endpoint};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    PERSON_AUTHORIZED, declared_per_person, person_authorize, person_kept,
};
use support::person_renewals::{
    EXPIRING, SOON, UNCLAIMED, for_person, own_shown, person_refresh, person_removals, renewals,
};
use support::sso::location;
use support::{Client, TestApi};

/// The connector declared for each person, authorized by each of `people`
/// through their browser: their real tokens, renewable.
async fn authorized_by(
    paul: &Client<'_>,
    authority: &FakeAuthorizationServer,
    prefix: &str,
    people: &[&Client<'_>],
) -> (ProtectedMcp, String) {
    let (server, id) = declared_per_person(paul, authority, prefix).await;
    for person in people {
        let back = person_authorize(person, authority, &id).await;
        assert_eq!(location(&back), PERSON_AUTHORIZED);
    }
    (server, id)
}

/// The connector `name`'s issue for `served`'s runs: its message and
/// whether it is theirs to authorize.
async fn issue_for(
    connectors: &ConnectorService,
    served: Served,
    name: &str,
) -> Option<(String, bool)> {
    let tools = connectors.tools_for(tenant::ID, served).await.unwrap();
    tools
        .issues
        .into_iter()
        .find(|issue| issue.connector == name)
        .map(|issue| (issue.message, issue.to_authorize))
}

/// A date the account shows.
fn at(shown: &Value, field: &str) -> DateTime<chrono::FixedOffset> {
    shown[field]
        .as_str()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .unwrap_or_else(|| panic!("{field}: {shown}"))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_s_token_about_to_expire_is_renewed_for_their_runs_alone() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = authorized_by(&paul, &authority, "renouvsoi", &[&marc, &claire]).await;
    let connectors = &api.app.services.connectors;
    let (as_marc, as_claire) = (as_person(user::MARC), as_person(user::CLAIRE));

    // Marc's session started with his token, then due for renewal, as is
    // Claire's.
    let first = person_kept(&api, &id, user::MARC).await.expect("kept");
    let first_refresh = person_refresh(&api, &id, user::MARC).await;
    assert!(sees_for(connectors, "renouvsoi", as_marc).await);
    assert!(own_shown(&marc, &id).await["renewedAt"].is_null());
    for person in [user::MARC, user::CLAIRE] {
        for_person(&api, &id, person, SOON).await;
    }
    let claires = person_kept(&api, &id, user::CLAIRE).await.expect("kept");
    let declared = listed(&paul, &id).await["mcp"]["oauthVersion"].clone();

    // His run renews his alone, and its session starts with the new token.
    let asked = server.authorizations().len();
    assert!(sees_for(connectors, "renouvsoi", as_marc).await);
    assert_eq!(renewals(&authority), 1);
    let renewed = person_kept(&api, &id, user::MARC).await.expect("renewed");
    assert_ne!(renewed, first);
    assert_ne!(person_refresh(&api, &id, user::MARC).await, first_refresh);
    // Started with the new token; the old one only closing the session it
    // replaces, once.
    let (bearer, old) = (format!("Bearer {renewed}"), format!("Bearer {first}"));
    let since = server.authorizations().split_off(asked);
    let with = |token: &str| {
        since
            .iter()
            .filter(|sent| sent.as_deref() == Some(token))
            .count()
    };
    assert!(with(&bearer) > 0, "{since:?}");
    assert!(with(&old) <= 1, "{since:?}");
    assert_eq!(with(&bearer) + with(&old), since.len(), "{since:?}");
    assert_eq!(person_kept(&api, &id, user::CLAIRE).await, Some(claires));

    // Shown renewed in his account, lasting an hour; his next run renews
    // nothing.
    let shown = own_shown(&marc, &id).await;
    assert!(
        at(&shown, "expiresAt") > Utc::now() + Duration::minutes(50),
        "{shown}"
    );
    assert!(
        at(&shown, "renewedAt") > at(&shown, "authorizedAt"),
        "{shown}"
    );
    // Renewed, not given anew: a declaration's form opened before is still
    // on what is there (ADR-176).
    let now = listed(&paul, &id).await["mcp"]["oauthVersion"].clone();
    assert_eq!(now, declared);
    assert!(sees_for(connectors, "renouvsoi", as_marc).await);
    assert_eq!(renewals(&authority), 1);
    // Claire's own run renews hers.
    assert!(sees_for(connectors, "renouvsoi", as_claire).await);
    assert_eq!(renewals(&authority), 2);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_processes_renew_a_person_s_once() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = authorized_by(&paul, &authority, "ensemblesoi", &[&marc]).await;
    for_person(&api, &id, user::MARC, SOON).await;
    let worker = another_process(&api);
    let here = &api.app.services.connectors;
    let as_marc = as_person(user::MARC);
    let (seen_here, seen_there) = tokio::join!(
        sees_for(here, "ensemblesoi", as_marc),
        sees_for(&worker, "ensemblesoi", as_marc)
    );
    assert!(seen_here && seen_there);
    assert_eq!(renewals(&authority), 1);
    let access = person_kept(&api, &id, user::MARC).await.expect("kept");
    assert!(authority.accepts(&access, &server.url));
    worker.close().await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_out_of_reach_is_said_to_them_and_a_gone_grant_removes_theirs() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = authorized_by(&paul, &authority, "refussoi", &[&marc, &claire]).await;
    let connectors = &api.app.services.connectors;
    let as_marc = as_person(user::MARC);
    let name = listed(&paul, &id).await["name"]
        .as_str()
        .expect("named")
        .to_string();
    let status = |listed: Value| (listed["status"].clone(), listed["lastError"].clone());
    let server_before = status(listed(&paul, &id).await);

    // Its token endpoint unreachable: his kept, too short to start a
    // session, the failure shown in his account without a token; his run
    // told to authorize it in his account.
    alter(&api, &id, &token_endpoint("http://127.0.0.1:1/token")).await;
    for_person(&api, &id, user::MARC, EXPIRING).await;
    let before = person_kept(&api, &id, user::MARC).await.expect("kept");
    let refresh = person_refresh(&api, &id, user::MARC)
        .await
        .expect("renewable");
    assert!(!sees_for(connectors, "refussoi", as_marc).await);
    let issue = issue_for(connectors, as_marc, &name).await;
    assert_eq!(issue, Some((TO_AUTHORIZE.to_string(), true)));
    assert_eq!(
        person_kept(&api, &id, user::MARC).await,
        Some(before.clone())
    );
    let failed = own_shown(&marc, &id).await;
    let failure = failed["renewalFailure"].as_str().unwrap_or_default();
    assert!(failure.contains("127.0.0.1:1"), "{failed}");
    for secret in [before.as_str(), refresh.as_str(), CLIENT_SECRET] {
        assert!(!failure.contains(secret), "{failure}");
    }
    // Not his to see in Claire's account, nor on the server's status.
    assert!(own_shown(&claire, &id).await["renewalFailure"].is_null());
    assert_eq!(status(listed(&paul, &id).await), server_before);

    // Reached again once its claim is over: renewed, the failure gone.
    let issued_at = format!("{}/token", authority.issuer);
    alter(&api, &id, &token_endpoint(&issued_at)).await;
    for_person(&api, &id, user::MARC, UNCLAIMED).await;
    assert!(sees_for(connectors, "refussoi", as_marc).await);
    let recovered = own_shown(&marc, &id).await;
    assert!(recovered["renewalFailure"].is_null(), "{recovered}");
    assert!(recovered["renewedAt"].is_string(), "{recovered}");
    assert_eq!(connectors.person_sessions().await, 1);

    // Expiring again, the grant revoked there: his removed by the system,
    // the reason without a token, his session released, his run told.
    let renewed = person_kept(&api, &id, user::MARC).await.expect("kept");
    for_person(&api, &id, user::MARC, EXPIRING).await;
    authority.revoke_all();
    assert!(!sees_for(connectors, "refussoi", as_marc).await);
    assert!(person_kept(&api, &id, user::MARC).await.is_none());
    assert_eq!(connectors.person_sessions().await, 0);
    let removals = person_removals(&api, &id).await;
    let (reason, whose) = removals.last().expect("audited");
    assert!(
        reason.starts_with("Autorisation retirée au renouvellement")
            && reason.contains("(invalid_grant)"),
        "{reason}"
    );
    assert!(!reason.contains(&renewed));
    assert_eq!(whose, &json!({ "person": user::MARC }));
    let issue = issue_for(connectors, as_marc, &name).await;
    assert_eq!(issue, Some((TO_AUTHORIZE.to_string(), true)));
    // Claire's, not due, kept.
    assert!(person_kept(&api, &id, user::CLAIRE).await.is_some());
    assert_eq!(removals.len(), 1);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
