//! An MCP server's OAuth access token renewed (ADR-171): one about to
//! expire is renewed with its refresh token before the session starts, the
//! rotated refresh token kept, the server sent the new token; a fresh one is
//! not renewed again; two processes renewing it at once renew it once (a
//! rotated refresh token used twice would revoke the grant); a token
//! endpoint that does not answer delays one run alone, and is not asked
//! again before its claim ends; a server that cannot be reached leaves the
//! authorization; one saying the grant is gone, or granting tokens the
//! platform could not keep, removes it, audited by the system, never a
//! token; a renewal is never asked under another declaration.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use futures::future::join_all;
use mimorya_connectors::mcp::OAUTH_EXPIRED;
use mimorya_connectors::mcp::oauth::OAUTH_TO_AUTHORIZE;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::connector_oauth::{AUTHORIZED, authorize, declared, kept_tokens};
use support::connector_sessions::{alter, another_process, runs_see, said, sees, tested};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::sso::location;
use tokio::net::TcpListener;

/// The access token expiring in three minutes and a half: due for renewal,
/// still long enough to start a session.
const SOON: &str = "update connector_authorizations set access_expires_at = \
                    now() + interval '210 seconds' where connector_id = $1";
/// The access token expiring in two minutes: too short to start a session.
const EXPIRING: &str = "update connector_authorizations set access_expires_at = \
                        now() + interval '2 minutes' where connector_id = $1";
/// The renewal's claim over, as if its delay had passed.
const UNCLAIMED: &str =
    "update connector_authorizations set renewing_until = null where connector_id = $1";

/// The connector's token endpoint moved to `to`.
fn token_endpoint(to: &str) -> String {
    format!(
        "update connector_instances set config = jsonb_set(config, \
         '{{oauth,tokenEndpoint}}', '\"{to}\"') where id = $1"
    )
}

/// A token endpoint that takes every connection and never answers: its
/// address, and how many connections it took.
async fn mute() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("port");
    let url = format!("http://{}/token", listener.local_addr().expect("address"));
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
            held.push(socket);
        }
    });
    (url, calls)
}

/// The system's removals of this connector's authorization: their reasons.
async fn removed_by_the_system(api: &TestApi, id: &str) -> Vec<String> {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let reasons: Vec<Option<String>> = sqlx::query_scalar(
        "select reason from audit_log where resource_id = $1 and actor_type = 'system' \
         and action = 'connector.oauth_authorization_removed' order by id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    reasons.into_iter().map(Option::unwrap_or_default).collect()
}

/// How many renewals the authorization server was asked for.
fn renewals(authority: &FakeAuthorizationServer) -> usize {
    authority
        .requests()
        .iter()
        .filter(|asked| {
            asked.endpoint == "token"
                && asked.params.get("grant_type").map(String::as_str) == Some("refresh_token")
        })
        .count()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_token_about_to_expire_is_renewed_before_the_session_starts() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "renouvele").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (first, first_refresh) = kept_tokens(&api, &id).await.expect("kept");
    alter(&api, &id, SOON).await;
    let asked = server.authorizations().len();
    assert!(runs_see(&api, "renouvele").await);
    let (second, second_refresh) = kept_tokens(&api, &id).await.expect("renewed");
    assert_ne!(second, first);
    assert_ne!(second_refresh, first_refresh);
    assert_eq!(renewals(&authority), 1);
    let bearer = format!("Bearer {second}");
    let since = server.authorizations().split_off(asked);
    assert!(!since.is_empty());
    assert!(
        since
            .iter()
            .all(|given| given.as_deref() == Some(bearer.as_str()))
    );
    // Shown lasting an hour; renewed, the next run renews nothing.
    let shown = listed(&paul, &id).await;
    let expires = shown["mcp"]["oauth"]["authorization"]["expiresAt"]
        .as_str()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .expect("expiry");
    assert!(expires > Utc::now() + Duration::minutes(50), "{shown}");
    assert!(runs_see(&api, "renouvele").await);
    assert_eq!(renewals(&authority), 1);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn two_processes_renew_it_once() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "ensemble").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    alter(&api, &id, SOON).await;
    let worker = another_process(&api);
    let here = &api.app.services.connectors;
    let (seen_here, seen_there) = tokio::join!(sees(here, "ensemble"), sees(&worker, "ensemble"));
    assert!(seen_here && seen_there);
    assert_eq!(renewals(&authority), 1);
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    assert!(authority.accepts(&access, &server.url));
    worker.close().await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_absent_server_keeps_the_authorization_and_a_gone_grant_removes_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "refus").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    // Its token endpoint unreachable: kept, too short to start a session.
    alter(&api, &id, &token_endpoint("http://127.0.0.1:1/token")).await;
    alter(&api, &id, EXPIRING).await;
    let (before, _) = kept_tokens(&api, &id).await.expect("kept");
    let expiring = tested(&paul, &id).await;
    assert!(said(&expiring).contains(OAUTH_EXPIRED), "{expiring}");
    assert_eq!(kept_tokens(&api, &id).await.expect("kept").0, before);
    // Reached again once its claim is over, the grant revoked there:
    // removed, by the system, the reason without a token.
    let issued_at = format!("{}/token", authority.issuer);
    alter(&api, &id, &token_endpoint(&issued_at)).await;
    alter(&api, &id, UNCLAIMED).await;
    authority.revoke_all();
    let refused = tested(&paul, &id).await;
    assert!(said(&refused).contains(OAUTH_TO_AUTHORIZE), "{refused}");
    assert!(kept_tokens(&api, &id).await.is_none());
    let reasons = removed_by_the_system(&api, &id).await;
    let reason = reasons.last().expect("audited");
    assert!(
        reason.starts_with("Autorisation retirée au renouvellement")
            && reason.contains("(invalid_grant)"),
        "{reason}"
    );
    assert!(!reason.contains(&before));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_mute_token_endpoint_delays_one_run_alone() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "muet").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (endpoint, calls) = mute().await;
    alter(&api, &id, &token_endpoint(&endpoint)).await;
    alter(&api, &id, SOON).await;
    let connectors = &api.app.services.connectors;
    let run = || async {
        let started = Instant::now();
        (sees(connectors, "muet").await, started.elapsed())
    };
    // Five runs at once: all served with the current token, one alone
    // waiting for the token endpoint, asked once.
    let runs = join_all((0..5).map(|_| run())).await;
    assert!(runs.iter().all(|(seen, _)| *seen));
    let slow = std::time::Duration::from_secs(5);
    let waited = runs.iter().filter(|(_, took)| *took > slow).count();
    assert_eq!(waited, 1, "{runs:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    // Not asked again while its claim holds.
    let (seen, took) = run().await;
    assert!(seen && took < slow, "{took:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn tokens_it_cannot_keep_remove_it_and_another_declaration_asks_nothing() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "garde").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    // Kept under another issuer than the one declared: never renewed there.
    let (before, _) = kept_tokens(&api, &id).await.expect("kept");
    let elsewhere = "update connector_authorizations set issuer = \
                     'https://autre-emetteur.example', access_expires_at = \
                     now() + interval '210 seconds' where connector_id = $1";
    alter(&api, &id, elsewhere).await;
    assert!(!runs_see(&api, "garde").await);
    assert_eq!(renewals(&authority), 0);
    assert_eq!(kept_tokens(&api, &id).await.expect("kept").0, before);
    // Authorized again, renewed into tokens too short-lived to keep: removed.
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    authority.set(|settings| settings.lifetime = Some(60));
    alter(&api, &id, SOON).await;
    let lost = tested(&paul, &id).await;
    assert!(said(&lost).contains(OAUTH_TO_AUTHORIZE), "{lost}");
    assert!(kept_tokens(&api, &id).await.is_none());
    let reasons = removed_by_the_system(&api, &id).await;
    let reason = reasons.last().expect("audited");
    assert!(reason.contains("moins de 5 minutes"), "{reason}");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
