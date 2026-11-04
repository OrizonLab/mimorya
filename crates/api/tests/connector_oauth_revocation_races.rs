//! What a revocation of an MCP server's tokens (ADR-172) meets besides the
//! request that withdraws them: that request abandoned while its server
//! waits, the revocation still goes on to its bound and is audited; a
//! renewal in flight when the authorization is withdrawn, the tokens it
//! brings back, which no one keeps, are revoked as well; not when another
//! authorization took its place, which revoking them could take with them
//! at some servers.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_connectors::testing::{AuthRequest, FakeAuthorizationServer};
use mimorya_core::demo::user;
use support::TestApi;
use support::connector_oauth::{
    AUTHORIZED, authorization_entries, authorize, declared, kept_tokens,
};
use support::connector_sessions::{alter, runs_see};
use support::mcp::PRIVATE_DEPLOYMENT;
use support::sso::location;
use tokio::time::{Instant, sleep, timeout};

/// The access token expiring in three minutes and a half: due for renewal.
const SOON: &str = "update connector_authorizations set access_expires_at = \
                    now() + interval '210 seconds' where connector_id = $1";

/// The revocations audited for this connector: outcome and reason.
async fn revocations(api: &TestApi, id: &str) -> Vec<(String, Option<String>)> {
    authorization_entries(api, id)
        .await
        .into_iter()
        .filter(|(action, ..)| action == "connector.oauth_authorization_revoked")
        .map(|(_, outcome, reason, _)| (outcome, reason))
        .collect()
}

/// The requests the authorization server received at `endpoint`.
fn asked_at(authority: &FakeAuthorizationServer, endpoint: &str) -> Vec<AuthRequest> {
    let requests = authority.requests();
    requests
        .into_iter()
        .filter(|asked| asked.endpoint == endpoint)
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_withdrawal_its_request_abandons_is_revoked_and_audited_all_the_same() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let mute = FakeAuthorizationServer::start().await;
    mute.set(|settings| settings.mute_revocation = true);
    let (_, id) = declared(&paul, &mute, "abandon").await;
    assert_eq!(location(&authorize(&paul, &mute, &id).await), AUTHORIZED);
    // The request given up while the server keeps the revocation waiting.
    let path = format!("/admin/connectors/{id}/oauth/authorization");
    let given_up = timeout(Duration::from_secs(1), paul.delete(&path)).await;
    assert!(given_up.is_err(), "answered before the revocation's bound");
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(asked_at(&mute, "revoke").len(), 1);
    // Its bound reached all the same, and audited.
    let deadline = Instant::now() + Duration::from_secs(20);
    let audited = loop {
        let audited = revocations(&api, &id).await;
        if !audited.is_empty() || Instant::now() > deadline {
            break audited;
        }
        sleep(Duration::from_millis(250)).await;
    };
    let (outcome, reason) = audited.first().expect("audited");
    assert_eq!(outcome, "failure");
    assert!(
        reason.as_deref().unwrap_or_default().contains("à temps"),
        "{reason:?}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_tokens_of_a_renewal_in_flight_when_withdrawn_are_revoked() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "envol").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (access, refresh) = kept_tokens(&api, &id).await.expect("kept");
    let refresh = refresh.expect("renewable");
    // Its renewal issued at the server, its answer kept back meanwhile.
    alter(&api, &id, SOON).await;
    authority.set(|settings| settings.held_renewals = Some(Duration::from_secs(2)));
    let path = format!("/admin/connectors/{id}/oauth/authorization");
    let withdrawn = async {
        while asked_at(&authority, "token").len() < 2 {
            sleep(Duration::from_millis(20)).await;
        }
        paul.delete(&path).await
    };
    let (seen, withdrawn) = tokio::join!(runs_see(&api, "envol"), withdrawn);
    assert!(!seen);
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    // The withdrawn tokens revoked, then the renewal's, which no one keeps.
    let revoked = asked_at(&authority, "revoke");
    let tokens: Vec<_> = revoked.iter().map(|r| r.params["token"].as_str()).collect();
    assert_eq!(tokens.len(), 4, "{tokens:?}");
    assert_eq!(tokens[..2], [refresh.as_str(), access.as_str()]);
    let hints: Vec<_> = revoked[2..]
        .iter()
        .map(|r| r.params["token_type_hint"].as_str())
        .collect();
    assert_eq!(hints, ["refresh_token", "access_token"]);
    assert!(tokens[2..].iter().all(|t| *t != refresh && *t != access));
    assert!(kept_tokens(&api, &id).await.is_none());
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_renewal_in_flight_when_authorized_again_revokes_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "relais").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    alter(&api, &id, SOON).await;
    authority.set(|settings| settings.held_renewals = Some(Duration::from_secs(2)));
    let again = async {
        while asked_at(&authority, "token").len() < 2 {
            sleep(Duration::from_millis(20)).await;
        }
        authorize(&paul, &authority, &id).await
    };
    let (_, again) = tokio::join!(runs_see(&api, "relais"), again);
    assert_eq!(location(&again), AUTHORIZED);
    // The new authorization kept; nothing revoked at its server.
    assert!(asked_at(&authority, "revoke").is_empty());
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    assert!(authority.accepts(&access, &server.url));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
