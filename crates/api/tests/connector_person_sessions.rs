//! A run on a server each person authorizes (ADR-174): given the tools of the
//! instance of the person it serves, started with their own authorization,
//! never another's; one instance per person, kept for their next runs,
//! started again once they authorize anew, released when they withdraw it.
//! A person who has not authorized it, or whose token expired or was
//! refused, is told to, in their account, and nothing is sent for them
//! unless their token is; nor for one who may not use connectors; an
//! evaluation serves no one and withholds it. A withdrawal, or an account
//! disabled, releases the session.
//! None of this is written on the connector's status, which every
//! administrator sees.

mod support;

use axum::http::StatusCode;
use mimorya_agent::llm::LlmMessage;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::connectors::{
    ConnectorService, NOT_FOR_THEM, Served, TO_AUTHORIZE, WITHHELD_PERSONAL,
};
use mimorya_connectors::testing::{FakeAuthorizationServer, ProtectedMcp};
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::accounts::{activate, create_account};
use support::connector_sessions::{alter, as_person, sees_for, token_endpoint};
use support::mcp::{PRIVATE_DEPLOYMENT, listed};
use support::person_authorizations::{
    declared_per_person, person_authorized, person_authorized_until,
};
use support::runs::start_run;
use support::{Client, TestApi};

const NAME: &str = "Fichiers chacun";
/// The other test's, apart: one failing leaves the other alone.
const TO_AUTHORIZE_NAME: &str = "Fichiers à autoriser";

/// How many of the server's requests carried `token`.
fn sent(server: &ProtectedMcp, token: &str) -> usize {
    let bearer = format!("Bearer {token}");
    server
        .authorizations()
        .iter()
        .filter(|header| header.as_deref() == Some(bearer.as_str()))
        .count()
}

/// The connector's issue for `served`'s runs: its message and whether it
/// is to authorize.
async fn issue_for(connectors: &ConnectorService, served: Served) -> Option<(String, bool)> {
    let tools = connectors.tools_for(tenant::ID, served).await.unwrap();
    tools
        .issues
        .into_iter()
        .find(|issue| issue.connector == NAME)
        .map(|issue| (issue.message, issue.to_authorize))
}

/// The connector's status as listed, and why it last failed.
async fn status(admin: &Client<'_>, id: &str) -> (Value, Value) {
    let connector = listed(admin, id).await;
    (connector["status"].clone(), connector["lastError"].clone())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn each_person_s_runs_use_their_own_authorization() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "chacun").await;
    let renamed = paul
        .patch(&format!("/admin/connectors/{id}"), json!({ "name": NAME }))
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    let resource = listed(&paul, &id).await["mcp"]["oauth"]["resource"]
        .as_str()
        .unwrap()
        .to_string();
    let before = status(&paul, &id).await;
    let connectors = &api.app.services.connectors;
    let (as_marc, as_claire, as_lea) = (
        as_person(user::MARC),
        as_person(user::CLAIRE),
        as_person(user::LEA),
    );

    // Marc's and Claire's runs, each with their own.
    let marcs = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    let claires = person_authorized(&api, &id, user::CLAIRE, &authority, &resource).await;
    assert!(sees_for(connectors, "chacun", as_marc).await);
    let marc_started = sent(&server, &marcs);
    assert!(marc_started > 0);
    assert!(sees_for(connectors, "chacun", as_claire).await);
    assert!(sent(&server, &claires) > 0);
    // Kept apart: Marc's next run takes his, started once.
    assert!(sees_for(connectors, "chacun", as_marc).await);
    assert_eq!(sent(&server, &marcs), marc_started);

    // Léa has not authorized it: nothing sent for her, told to.
    let requests = server.authorizations().len();
    assert!(!sees_for(connectors, "chacun", as_lea).await);
    assert_eq!(server.authorizations().len(), requests);
    assert_eq!(
        issue_for(connectors, as_lea).await,
        Some((TO_AUTHORIZE.to_string(), true))
    );
    assert_eq!(issue_for(connectors, as_marc).await, None);
    // An evaluation serves no one: withheld, not to authorize.
    assert!(!sees_for(connectors, "chacun", Served::Evaluation).await);
    assert_eq!(
        issue_for(connectors, Served::Evaluation).await,
        Some((WITHHELD_PERSONAL.to_string(), false))
    );
    assert_eq!(server.authorizations().len(), requests);

    // Marc authorizes anew: his next run starts with his new token.
    assert_eq!(connectors.person_sessions().await, 2);
    let renewed = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    assert!(sees_for(connectors, "chacun", as_marc).await);
    assert!(sent(&server, &renewed) > 0);
    assert_eq!(connectors.person_sessions().await, 2);
    // He withdraws it: his session goes, he is told to authorize it again.
    let path = format!("/account/connector-authorizations/{id}");
    assert_eq!(marc.delete(&path).await.status, StatusCode::OK);
    assert_eq!(connectors.person_sessions().await, 1);
    assert_eq!(
        issue_for(connectors, as_marc).await,
        Some((TO_AUTHORIZE.to_string(), true))
    );

    // His own expired, its renewal out of reach, then refused at the
    // authorization server: never sent expired, refused when sent; his to
    // authorize again either way.
    alter(&api, &id, &token_endpoint("http://127.0.0.1:1/token")).await;
    let past = Some(chrono::Utc::now() - chrono::Duration::minutes(5));
    let expired = person_authorized_until(&api, &id, user::MARC, &authority, &resource, past).await;
    assert!(!sees_for(connectors, "chacun", as_marc).await);
    assert_eq!(sent(&server, &expired), 0);
    let issue = issue_for(connectors, as_marc).await;
    assert_eq!(issue, Some((TO_AUTHORIZE.to_string(), true)));
    let refused = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    authority.revoke_all();
    assert!(!sees_for(connectors, "chacun", as_marc).await);
    assert!(sent(&server, &refused) > 0);
    let issue = issue_for(connectors, as_marc).await;
    assert_eq!(issue, Some((TO_AUTHORIZE.to_string(), true)));
    // None of it is the server's status.
    assert_eq!(status(&paul, &id).await, before);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_starts_for_who_may_not_use_it_and_a_disabled_account_s_session_goes() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let authority = FakeAuthorizationServer::start().await;
    let (server, id) = declared_per_person(&paul, &authority, "partis").await;
    let resource = listed(&paul, &id).await["mcp"]["oauth"]["resource"]
        .as_str()
        .unwrap()
        .to_string();
    let connectors = &api.app.services.connectors;

    // Marc authorized it, then may no longer use connectors: his token is
    // not sent for tools he could not be given.
    let marcs = person_authorized(&api, &id, user::MARC, &authority, &resource).await;
    let barred = Served::Person {
        user: user::MARC,
        uses_connectors: false,
    };
    assert!(!sees_for(connectors, "partis", barred).await);
    assert_eq!(sent(&server, &marcs), 0);
    let tools = connectors.tools_for(tenant::ID, barred).await.unwrap();
    let issue = tools
        .issues
        .iter()
        .find(|issue| issue.connector_type == "mcp");
    assert!(issue.is_some_and(|issue| issue.message == NOT_FOR_THEM && !issue.to_authorize));
    assert_eq!(connectors.person_sessions().await, 0);

    // An account disabled: its session goes with its authorizations.
    let mut account = create_account(&paul, "session.partie").await;
    activate(&api, &mut account).await;
    person_authorized(&api, &id, account.id, &authority, &resource).await;
    assert!(sees_for(connectors, "partis", as_person(account.id)).await);
    assert_eq!(connectors.person_sessions().await, 1);
    let disabled = paul
        .patch(
            &format!("/admin/users/{}", account.id),
            json!({ "status": "disabled" }),
        )
        .await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(connectors.person_sessions().await, 0);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}

/// A run of `client`, once over: the user turn its model received, and the
/// connectors its stream announced as unavailable.
async fn run_of(api: &TestApi, client: &Client<'_>) -> (String, Option<Value>) {
    api.script(vec![ScriptedStep::text("C’est noté.")]);
    let run_id = start_run(client).await;
    api.idle().await;
    let requests = api.model.as_ref().unwrap().requests();
    let Some(LlmMessage::User { content }) = requests.last().and_then(|r| r.messages.last()) else {
        panic!("a user turn last");
    };
    let stream = client.get(&format!("/runs/{run_id}/events")).await.text();
    let announced = stream
        .split("\n\n")
        .find(|event| event.contains("event: connectors_unavailable"))
        .and_then(|event| event.lines().find_map(|l| l.strip_prefix("data: ")))
        .map(|data| serde_json::from_str(data).unwrap());
    (content.clone(), announced)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_tells_its_person_to_authorize_the_server_in_their_account() {
    let api = TestApi::start_with(&PRIVATE_DEPLOYMENT).await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let lea = api.sign_in(user::LEA).await;
    let authority = FakeAuthorizationServer::start().await;
    let (_, id) = declared_per_person(&paul, &authority, "aautoriser").await;
    let renamed = paul
        .patch(
            &format!("/admin/connectors/{id}"),
            json!({ "name": TO_AUTHORIZE_NAME }),
        )
        .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.text());
    let resource = listed(&paul, &id).await["mcp"]["oauth"]["resource"]
        .as_str()
        .unwrap()
        .to_string();
    person_authorized(&api, &id, user::MARC, &authority, &resource).await;

    // Léa's run: told, to send her to her account.
    let (turn, announced) = run_of(&api, &lea).await;
    assert!(
        turn.contains(&format!(
            "chaque personne autorise pour soi « {TO_AUTHORIZE_NAME} »"
        )),
        "{turn}"
    );
    assert!(turn.contains("« Mon compte »"), "{turn}");
    assert_eq!(
        announced,
        Some(json!({ "connectors": [TO_AUTHORIZE_NAME], "toAuthorize": [TO_AUTHORIZE_NAME] }))
    );
    // Marc's: his tools given, nothing told.
    let (turn, announced) = run_of(&api, &marc).await;
    assert!(!turn.contains(TO_AUTHORIZE_NAME), "{turn}");
    assert_eq!(announced, None);
    assert_eq!(
        paul.delete(&format!("/admin/connectors/{id}")).await.status,
        StatusCode::OK
    );
}
