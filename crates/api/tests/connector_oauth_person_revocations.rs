//! The people's own OAuth authorizations of an MCP server (ADR-174) go with
//! the declaration they were given under, their tokens revoked at the
//! authorization server (ADR-172), audited together without a token nor a
//! name: when OAuth is declared again or withdrawn, when the server's
//! address changes and when the connector is deleted; another connector's
//! stay. Taken all the same, a person's tokens are never sent under another
//! declaration than theirs, nor under one that can no longer be read: said
//! as failures, counted with the others.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::connectors::REVOCATION_BOUND;
use mimorya_connectors::testing::{
    CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, OAuthQuirk, serve_oauth_mcp_http,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::connector_oauth::declared;
use support::connector_sessions::alter;
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    last_metadata, person_authorized, person_count, person_revocations,
};
use support::{Client, TestApi};

/// A server each person authorizes, declared, with Marc's and Léa's own
/// authorizations: its id, its resource and their tokens.
async fn authorized_by_two(
    api: &TestApi,
    paul: &Client<'_>,
    authority: &FakeAuthorizationServer,
    prefix: &str,
) -> (String, String, [String; 2]) {
    let (_, id) = declared(paul, authority, prefix).await;
    let each = paul
        .put(
            &format!("/admin/connectors/{id}/oauth/holder"),
            json!({ "perPerson": true }),
        )
        .await;
    assert_eq!(each.status, StatusCode::OK, "{}", each.text());
    let resource = listed(paul, &id).await["mcp"]["oauth"]["resource"]
        .as_str()
        .unwrap()
        .to_string();
    let marc = person_authorized(api, &id, user::MARC, authority, &resource).await;
    let lea = person_authorized(api, &id, user::LEA, authority, &resource).await;
    (id, resource, [marc, lea])
}

/// The tokens no longer good, revoked and audited together.
async fn revoked(
    api: &TestApi,
    authority: &FakeAuthorizationServer,
    id: &str,
    resource: &str,
    tokens: &[String; 2],
) {
    assert_eq!(person_count(api, id).await, 0);
    for token in tokens {
        assert!(!authority.accepts(token, resource));
    }
    let audited = person_revocations(api, id).await;
    let last = audited.last().expect("audited");
    assert_eq!(last.0, "success", "{audited:?}");
    assert_eq!(last.2, json!({ "revoked": 2, "failed": 0 }));
    let said = format!("{audited:?}");
    assert!(tokens.iter().all(|token| !said.contains(token.as_str())));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_people_s_authorizations_go_with_their_declaration() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let declaration = json!({
        "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET, "perPerson": true,
    });
    // Another connector's stay whatever happens to these.
    let (other, other_resource, other_tokens) =
        authorized_by_two(&api, &paul, &authority, "autre").await;

    // OAuth declared again.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "redit").await;
    let again = paul
        .put(
            &format!("/admin/connectors/{id}/oauth"),
            declaration.clone(),
        )
        .await;
    assert_eq!(again.status, StatusCode::OK, "{}", again.text());
    assert_eq!(again.json()["personAuthorizations"], 0);
    revoked(&api, &authority, &id, &resource, &tokens).await;
    let declared = last_metadata(&api, &id, "connector.oauth_declared").await;
    assert_eq!(declared["perPerson"], true);
    assert_eq!(declared["personAuthorizationsRemoved"], 2);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // OAuth withdrawn.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "retire").await;
    let withdrawn = paul.delete(&format!("/admin/connectors/{id}/oauth")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    revoked(&api, &authority, &id, &resource, &tokens).await;
    let removed = last_metadata(&api, &id, "connector.oauth_removed").await;
    assert_eq!(removed["personAuthorizationsRemoved"], 2);
    assert!(!removed.to_string().contains(&user::MARC.to_string()));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // The server's address changed.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "demenage").await;
    let elsewhere = serve_oauth_mcp_http(&authority, OAuthQuirk::None).await;
    let moved = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "url": elsewhere.url }),
        )
        .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.text());
    revoked(&api, &authority, &id, &resource, &tokens).await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // The connector deleted.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "supprime").await;
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
    revoked(&api, &authority, &id, &resource, &tokens).await;

    // The other connector's are still kept and good.
    assert_eq!(person_count(&api, &other).await, 2);
    assert!(
        other_tokens
            .iter()
            .all(|t| authority.accepts(t, &other_resource))
    );
    assert!(person_revocations(&api, &other).await.is_empty());
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{other}"))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(person_count(&api, &other).await, 0);
}

/// How many revocations the authorization server was asked for.
fn asked(authority: &FakeAuthorizationServer) -> usize {
    let requests = authority.requests();
    requests.iter().filter(|r| r.endpoint == "revoke").count()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_s_tokens_are_never_sent_under_another_declaration_or_an_unreadable_one() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;

    // Léa's given under another issuer: never sent, Marc's revoked.
    let (id, resource, [marc, lea]) = authorized_by_two(&api, &paul, &authority, "ailleurs").await;
    let elsewhere = format!(
        "update connector_person_authorizations set issuer = 'https://autre-emetteur.example' \
         where connector_id = $1 and user_id = '{}'",
        user::LEA
    );
    alter(&api, &id, &elsewhere).await;
    let before = asked(&authority);
    let withdrawn = paul.delete(&format!("/admin/connectors/{id}/oauth")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    assert_eq!(person_count(&api, &id).await, 0, "taken all the same");
    assert_eq!(asked(&authority) - before, 1, "Marc's alone was sent");
    assert!(!authority.accepts(&marc, &resource));
    assert!(
        authority.accepts(&lea, &resource),
        "Léa's left at its server"
    );
    let audited = person_revocations(&api, &id).await;
    let (outcome, reason, counts) = audited.last().expect("audited");
    assert_eq!(outcome, "failure");
    assert_eq!(counts, &json!({ "revoked": 1, "failed": 1 }));
    let reason = reason.as_deref().unwrap_or_default();
    assert!(reason.contains("autre déclaration"), "{reason}");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );

    // A declaration no longer readable: taken, nothing sent, said as such.
    let (id, resource, tokens) = authorized_by_two(&api, &paul, &authority, "illisible").await;
    let spoilt = "update connector_instances set config = config || \
                  '{\"oauth\": {\"issuer\": 1}}' where id = $1";
    alter(&api, &id, spoilt).await;
    let before = asked(&authority);
    let withdrawn = paul.delete(&format!("/admin/connectors/{id}/oauth")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    assert_eq!(person_count(&api, &id).await, 0);
    assert_eq!(asked(&authority), before, "nothing sent");
    assert!(
        tokens
            .iter()
            .all(|token| authority.accepts(token, &resource))
    );
    let audited = person_revocations(&api, &id).await;
    let (outcome, reason, counts) = audited.last().expect("audited");
    assert_eq!(outcome, "failure");
    assert_eq!(counts, &json!({ "revoked": 0, "failed": 2 }));
    let reason = reason.as_deref().unwrap_or_default();
    assert!(reason.contains("Déclaration OAuth illisible"), "{reason}");
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_request_does_not_wait_for_every_person_s_revocation() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (id, resource, _) = authorized_by_two(&api, &paul, &authority, "muet").await;
    for person in [user::PAUL, user::CLAIRE, user::JULIEN] {
        person_authorized(&api, &id, person, &authority, &resource).await;
    }
    // Its revocation endpoint never answers: five people, four at a time,
    // ten seconds each, twenty in all.
    authority.set(|settings| settings.mute_revocation = true);
    let started = std::time::Instant::now();
    let withdrawn = paul.delete(&format!("/admin/connectors/{id}/oauth")).await;
    let waited = started.elapsed();
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    // Waited for the bound of one revocation and five seconds (ADR-174),
    // and two seconds for the rest of the request.
    let ceiling = REVOCATION_BOUND + std::time::Duration::from_secs(5 + 2);
    assert!(waited < ceiling, "{waited:?}");
    assert_eq!(person_count(&api, &id).await, 0, "taken before the answer");
    // The revocations go on after the answer, and are audited.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let audited = loop {
        let audited = person_revocations(&api, &id).await;
        if !audited.is_empty() || std::time::Instant::now() > deadline {
            break audited;
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    };
    let (outcome, reason, counts) = audited.last().expect("audited after the answer");
    assert_eq!(outcome, "failure");
    assert_eq!(counts, &json!({ "revoked": 0, "failed": 5 }));
    assert!(
        reason.as_deref().unwrap_or_default().contains("à temps"),
        "{reason:?}"
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
