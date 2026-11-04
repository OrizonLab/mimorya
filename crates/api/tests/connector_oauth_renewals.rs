//! An MCP server's OAuth access token renewed (ADR-171): one about to
//! expire is renewed with its refresh token before the session starts, the
//! rotated refresh token kept, the server sent the new token; a fresh one is
//! not renewed again; two processes renewing it at once renew it once (a
//! rotated refresh token used twice would revoke the grant); a server that
//! cannot answer leaves the authorization, its token too short to start a
//! session; one that refuses removes it, audited by the system, never a
//! token.

mod support;

use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use mimorya_connectors::mcp::OAUTH_EXPIRED;
use mimorya_connectors::mcp::oauth::OAUTH_TO_AUTHORIZE;
use mimorya_connectors::testing::FakeAuthorizationServer;
use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::connector_oauth::{AUTHORIZED, authorize, declared, kept_tokens};
use support::connector_sessions::{alter, another_process, runs_see, said, sees, tested};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::sso::location;

/// The access token expiring in two minutes: due for renewal.
const SOON: &str = "update connector_authorizations set access_expires_at = \
                    now() + interval '2 minutes' where connector_id = $1";

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
async fn an_absent_server_keeps_the_authorization_and_a_refusal_removes_it() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "refus").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    // Its token endpoint unreachable: kept, too short to start a session.
    let endpoint = |to: &str| {
        format!(
            "update connector_instances set config = jsonb_set(config, \
             '{{oauth,tokenEndpoint}}', '\"{to}\"') where id = $1"
        )
    };
    alter(&api, &id, &endpoint("http://127.0.0.1:1/token")).await;
    alter(&api, &id, SOON).await;
    let (before, _) = kept_tokens(&api, &id).await.expect("kept");
    let expiring = tested(&paul, &id).await;
    assert!(said(&expiring).contains(OAUTH_EXPIRED), "{expiring}");
    assert_eq!(kept_tokens(&api, &id).await.expect("kept").0, before);
    // Reached again, the grant revoked there: the authorization removed.
    alter(&api, &id, &endpoint(&format!("{}/token", authority.issuer))).await;
    authority.revoke_all();
    let refused = tested(&paul, &id).await;
    assert!(said(&refused).contains(OAUTH_TO_AUTHORIZE), "{refused}");
    assert!(kept_tokens(&api, &id).await.is_none());
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<(String, Option<String>)> = sqlx::query_as(
        "select actor_type, reason from audit_log where resource_id = $1 \
         and action = 'connector.oauth_authorization_removed' order by id",
    )
    .bind(&id)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (actor, reason) = audited.last().expect("audited").clone();
    assert_eq!(actor, "system");
    let reason = reason.unwrap_or_default();
    assert!(reason.contains("(invalid_grant)"), "{reason}");
    assert!(!reason.contains(&before));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
