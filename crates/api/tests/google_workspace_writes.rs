//! The Google Workspace tools that write (ADR-123): offered once the
//! company lets them, taken back with it; an e-mail the agent would send
//! waits for its person's validation, then leaves exactly as validated,
//! with that person's token; an event, validated, is created in their
//! calendar and its attendees invited.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_connectors::google_workspace::GOOGLE_WORKSPACE_WRITE_TOOLS;
use mimorya_connectors::testing::{FakeGoogle, SentMail};
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::account_links::{
    LINKED, google_person, insert_google_workspace_reading, insert_google_workspace_writing, link,
};
use support::identity_provider::MockIdentityProvider;
use support::mailbox::validated_tool_result;
use support::manager_approvals::listed;
use support::sso::{SSO_DEPLOYMENT, location};
use support::{TestApi, email};

async fn write_tools_offered(api: &TestApi) -> usize {
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    GOOGLE_WORKSPACE_WRITE_TOOLS
        .iter()
        .filter(|name| tools.tools.iter().any(|t| t.info().name == **name))
        .count()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn writing_tools_are_offered_while_the_company_lets_them() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_reading(&api, &idp, &google).await;
    assert_eq!(write_tools_offered(&api).await, 0);
    let paul = api.sign_in(user::PAUL).await;
    let path = format!("/admin/connectors/{connector}");
    for (write_tools, offered) in [(true, 2), (false, 0)] {
        let changed = paul
            .patch(&path, json!({ "writeTools": write_tools }))
            .await;
        assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
        assert_eq!(write_tools_offered(&api).await, offered);
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_email_leaves_once_validated_exactly_as_validated() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_writing(&api, &idp, &google).await;
    let claire = api.sign_in(user::CLAIRE).await;
    let address = email(user::CLAIRE);
    let domain = address.rsplit_once('@').unwrap().1.to_string();
    idp.sign_in_as(google_person("g-claire", &address, Some(&domain)));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    let input = json!({
        "to": ["paul@exemple.fr"],
        "subject": "Planning de novembre",
        "body": "Bonjour Paul,\n\nLe planning est prêt.",
    });
    api.script(vec![
        ScriptedStep::calls("", vec![call("google_gmail_send", input.clone())]),
        ScriptedStep::dynamic(|request| {
            let results = last_tool_results(request);
            assert!(!results[0].is_error, "{}", results[0].content);
            ScriptedReply::text("L’e-mail est parti.")
        }),
    ]);
    let conversation = claire.post("/conversations", json!({})).await.json();
    let id = conversation["conversation"]["id"].as_str().unwrap();
    let sent = claire
        .post(
            &format!("/conversations/{id}/messages"),
            json!({ "content": "Envoie le planning à Paul." }),
        )
        .await;
    assert_eq!(sent.status, StatusCode::ACCEPTED, "{}", sent.text());
    api.idle().await;
    let run = sent.json()["runId"].as_str().unwrap().to_string();
    let request = listed(&claire, &run).await.expect("a request to validate");
    assert_eq!(request["toolName"], "google_gmail_send");
    assert_eq!(request["risk"], "external");
    // The exact parameters executed, its copy list empty.
    let mut exact = input.clone();
    exact["cc"] = json!([]);
    assert_eq!(request["payload"], exact);
    let summary = request["actionSummary"].as_str().unwrap();
    assert!(summary.contains("« Planning de novembre »") && summary.contains("paul@exemple.fr"));
    assert!(summary.contains("Google Workspace"), "{summary}");
    assert!(google.requests().is_empty(), "nothing leaves before");

    let decision = format!("/approvals/{}/decision", request["id"].as_str().unwrap());
    let decided = claire.post(&decision, json!({ "approved": true })).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
    let requests = google.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/users/me/messages/send");
    assert_eq!(requests[0].token, "acces-g-claire-1");
    let raw = requests[0].body.as_ref().unwrap()["raw"].as_str().unwrap();
    let mail = SentMail::read(raw).unwrap();
    assert_eq!(mail.header("To"), Some("paul@exemple.fr"));
    assert_eq!(mail.header("Subject"), Some("Planning de novembre"));
    assert_eq!(mail.text, "Bonjour Paul,\r\n\r\nLe planning est prêt.");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_validated_event_is_created_in_the_persons_calendar() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let google = FakeGoogle::start().await;
    let connector = insert_google_workspace_writing(&api, &idp, &google).await;
    let marc = api.sign_in(user::MARC).await;
    let address = email(user::MARC);
    let domain = address.rsplit_once('@').unwrap().1.to_string();
    idp.sign_in_as(google_person("g-marc", &address, Some(&domain)));
    assert_eq!(location(&link(&marc, &idp, connector).await), LINKED);
    let input = json!({
        "subject": "Visite du chantier",
        "start": "2026-11-03T09:30:00",
        "durationMinutes": 60,
        "attendees": ["paul@exemple.fr"],
    });
    let created = validated_tool_result(&api, &marc, "google_calendar_event_create", input).await;
    assert_eq!(created["cree"], true, "{created}");
    assert_eq!(created["id"], "g-marc-nouveau");
    let requests = google.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].token, "acces-g-marc-1");
    assert_eq!(requests[0].param("sendUpdates"), Some("all"));
    let event = requests[0].body.as_ref().unwrap();
    assert_eq!(event["start"]["dateTime"], "2026-11-03T08:30:00Z");
    assert_eq!(event["attendees"], json!([{ "email": "paul@exemple.fr" }]));
}
