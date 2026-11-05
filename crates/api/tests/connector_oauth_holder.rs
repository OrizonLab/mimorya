//! Who authorizes an MCP server declared with OAuth (ADR-174): the company,
//! or each person for themself, chosen at its declaration or changed in
//! place by a company-wide connector administrator. A server each person
//! authorizes never opens a session with the company's authorization,
//! even one kept, nor lets the company authorize it; changing who
//! authorizes takes the authorizations given under the former holder and
//! revokes their tokens. Refused to anyone who does not administer the
//! company's connectors, for a connector that is not the company's, for a server without a
//! readable declaration, and with a field it does not know.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::connectors::access_sealed_as;
use mimorya_connectors::mcp::oauth::OAUTH_EACH_PERSON;
use mimorya_connectors::testing::{CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer};
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::connector_authorizations::{
    NewConnectorAuthorization, put_connector_authorization,
};
use serde_json::json;
use support::TestApi;
use support::connector_oauth::{AUTHORIZED, authorize, declared, kept_tokens, try_start};
use support::connector_sessions::{said, tested};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{person_authorized, person_count, person_revocations};
use support::sso::location;

fn holder_path(id: &str) -> String {
    format!("/admin/connectors/{id}/oauth/holder")
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_server_each_person_authorizes_never_opens_with_the_company_s() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "chacun").await;
    // Declared again, each person authorizing it.
    let declared = paul
        .put(
            &format!("/admin/connectors/{id}/oauth"),
            json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET, "perPerson": true }),
        )
        .await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    assert_eq!(declared.json()["perPerson"], true);
    let oauth = &listed(&paul, &id).await["mcp"]["oauth"];
    assert_eq!(
        (
            oauth["perPerson"].clone(),
            oauth["personAuthorizations"].clone()
        ),
        (json!(true), json!(0))
    );

    // The company does not authorize it.
    let refused = try_start(&paul, &id).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(
        refused
            .text()
            .contains("Chaque personne autorise ce serveur")
    );
    // Nor does a company's authorization kept meanwhile (a process that
    // knew only the company's) open a session: never sent.
    let connector: uuid::Uuid = id.parse().unwrap();
    let resource = oauth["resource"].as_str().unwrap().to_string();
    let token = authority.issue(&resource);
    let cipher = api.app.services.config.secrets.clone().expect("master key");
    let sealed = cipher
        .seal(tenant::ID, &access_sealed_as(connector), &token)
        .unwrap();
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    put_connector_authorization(
        &mut tx,
        NewConnectorAuthorization {
            tenant_id: tenant::ID,
            connector_id: connector,
            issuer: &authority.issuer,
            client_id: CLIENT_ID,
            resource: &resource,
            scopes: "",
            access_token: &sealed,
            access_expires_at: None,
            refresh_token: None,
            authorized_by_user_id: user::PAUL,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let outcome = tested(&paul, &id).await;
    assert!(said(&outcome).contains(OAUTH_EACH_PERSON), "{outcome}");
    let sent = server.authorizations();
    assert!(
        sent.iter()
            .all(|header| header.as_deref() != Some(&format!("Bearer {token}")))
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn changing_who_authorizes_takes_what_the_former_holder_gave() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared(&paul, &authority, "passe").await;
    assert_eq!(
        location(&authorize(&paul, &authority, &id).await),
        AUTHORIZED
    );
    let (access, _) = kept_tokens(&api, &id).await.expect("kept");
    // Unchanged: nothing taken.
    let same = paul
        .put(&holder_path(&id), json!({ "perPerson": false }))
        .await;
    assert_eq!(same.status, StatusCode::OK, "{}", same.text());
    assert!(kept_tokens(&api, &id).await.is_some());

    // To each person: the company's authorization goes, revoked.
    let changed = paul
        .put(&holder_path(&id), json!({ "perPerson": true }))
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    assert!(kept_tokens(&api, &id).await.is_none());
    assert!(!authority.accepts(&access, &server.url));
    let oauth = &listed(&paul, &id).await["mcp"]["oauth"];
    assert_eq!(oauth["perPerson"], true);
    assert!(oauth["authorization"].is_null());
    let resource = oauth["resource"].as_str().unwrap().to_string();

    // Back to the company: every person's goes, revoked, audited together.
    let marc = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    let lea = person_authorized(&api, &id, user::LEA, &authority, &resource).await;
    assert_eq!(
        listed(&paul, &id).await["mcp"]["oauth"]["personAuthorizations"],
        2
    );
    let back = paul
        .put(&holder_path(&id), json!({ "perPerson": false }))
        .await;
    assert_eq!(back.status, StatusCode::OK, "{}", back.text());
    assert_eq!(person_count(&api, &id).await, 0);
    assert!(!authority.accepts(&marc, &resource) && !authority.accepts(&lea, &resource));
    let revocations = person_revocations(&api, &id).await;
    assert_eq!(revocations.len(), 1, "{revocations:?}");
    let (outcome, reason, counts) = &revocations[0];
    assert_eq!((outcome.as_str(), reason), ("success", &None));
    assert_eq!(counts, &json!({ "revoked": 2, "failed": 0 }));
    let audited = format!("{revocations:?}");
    assert!(!audited.contains(&marc) && !audited.contains(&user::MARC.to_string()));
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_a_company_wide_administrator_changes_who_authorizes() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared(&paul, &authority, "qui").await;
    let to_person = json!({ "perPerson": true });

    // Anyone who does not administer the company's connectors (an
    // administration role is given only for the whole company).
    let marc = api.sign_in(user::MARC).await;
    let refused = marc.put(&holder_path(&id), to_person.clone()).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    // A field it does not know.
    let unknown = paul
        .put(
            &holder_path(&id),
            json!({ "perPerson": true, "holder": "person" }),
        )
        .await;
    assert_eq!(
        unknown.status,
        StatusCode::BAD_REQUEST,
        "{}",
        unknown.text()
    );
    // A connector that is not there, or not this company's.
    let elsewhere = paul
        .put(
            &holder_path(&uuid::Uuid::new_v4().to_string()),
            to_person.clone(),
        )
        .await;
    assert_eq!(
        elsewhere.status,
        StatusCode::NOT_FOUND,
        "{}",
        elsewhere.text()
    );
    assert_eq!(listed(&paul, &id).await["mcp"]["oauth"]["perPerson"], false);

    // A server without OAuth declared: nothing to change.
    let withdrawn = paul.delete(&format!("/admin/connectors/{id}/oauth")).await;
    assert_eq!(withdrawn.status, StatusCode::OK, "{}", withdrawn.text());
    let undeclared = paul.put(&holder_path(&id), to_person).await;
    assert_eq!(
        undeclared.status,
        StatusCode::BAD_REQUEST,
        "{}",
        undeclared.text()
    );
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
