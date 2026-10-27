//! The Microsoft 365 tools that write (ADR-119): the company lets them, or
//! takes it back; its people are then asked to grant sending and writing
//! their calendar, and a link made before grants it by linking again; an
//! e-mail the agent would send waits for its person's validation, then
//! leaves exactly as validated, with that person's token.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::{ScriptedReply, ScriptedStep, call, last_tool_results};
use mimorya_api::services::account_links::PersonAccounts;
use mimorya_connectors::microsoft365::MICROSOFT365_WRITE_TOOLS;
use mimorya_connectors::testing::FakeGraph;
use mimorya_core::account_links::{CALENDARS_WRITE, MAIL_READ, MAIL_SEND, MICROSOFT365};
use mimorya_core::demo::{tenant, user};
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use serde_json::{Value, json};
use support::account_links::{
    DIRECTORY, LINKED, come_back, insert_microsoft365_reading, insert_microsoft365_writing, link,
    links, microsoft_person, start,
};
use support::identity_provider::MockIdentityProvider;
use support::manager_approvals::listed;
use support::refusals::latest_entry;
use support::rest::{add_rest, rest_body};
use support::sso::{SSO_DEPLOYMENT, location};
use support::{Client, TestApi, email};

const SECRET: &str = "Secret~Application.2026_abc";

async fn write_tools_offered(api: &TestApi) -> usize {
    let tools = api
        .app
        .services
        .connectors
        .tools_for(tenant::ID)
        .await
        .unwrap();
    let names: Vec<String> = tools.tools.iter().map(|t| t.info().name.clone()).collect();
    MICROSOFT365_WRITE_TOOLS
        .iter()
        .filter(|name| names.iter().any(|n| n == *name))
        .count()
}

async fn view_of(paul: &Client<'_>) -> Value {
    let body = paul.get("/admin/connectors").await.json();
    let found = body["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["type"] == "microsoft365");
    found.cloned().unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn the_company_lets_its_tools_write_and_takes_it_back() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    // One a previous test declared goes first.
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("delete from connector_instances where type = $1")
        .bind(MICROSOFT365)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let paul = api.sign_in(user::PAUL).await;
    let since = latest_entry(&api).await;
    let declaration = json!({
        "name": "Microsoft 365",
        "directory": DIRECTORY,
        "clientId": "0b1c2d3e-4f50-6172-8394-a5b6c7d8e9f0",
        "clientSecret": SECRET,
        "writeTools": true,
    });
    let created = paul
        .post("/admin/connectors/microsoft365", declaration)
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let path = format!(
        "/admin/connectors/{}",
        created.json()["id"].as_str().unwrap()
    );
    assert_eq!(view_of(&paul).await["microsoft365"]["writeTools"], true);
    assert_eq!(write_tools_offered(&api).await, 2);

    let taken_back = paul.patch(&path, json!({ "writeTools": false })).await;
    assert_eq!(taken_back.status, StatusCode::OK, "{}", taken_back.text());
    assert_eq!(view_of(&paul).await["microsoft365"]["writeTools"], false);
    assert_eq!(write_tools_offered(&api).await, 0);
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let audited: Vec<(String, Option<bool>)> = sqlx::query_as(
        "select action, (metadata->>'writeTools')::boolean from audit_log \
         where id > $1 and action like 'connector.%' order by id",
    )
    .bind(since)
    .fetch_all(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        audited,
        [
            ("connector.created".to_string(), Some(true)),
            ("connector.updated".to_string(), Some(false)),
        ]
    );

    // Another connector has none to let.
    let rest = add_rest(&paul, rest_body("CRM", "https://api.example.com/v1", None)).await;
    let rest_path = format!("/admin/connectors/{rest}");
    let refused = paul.patch(&rest_path, json!({ "writeTools": true })).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .contains("Seule l’application Microsoft 365")
    );
    for removed in [paul.delete(&rest_path).await, paul.delete(&path).await] {
        assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_made_before_grants_writing_by_linking_again() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_reading(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    let (url, cookie) = start(&claire, connector).await;
    assert!(!url.contains(MAIL_SEND), "{url}");
    come_back(&claire, &idp.authorize(&url), Some(&cookie)).await;
    assert_eq!(links(&claire).await[0]["linked"]["toGrant"], json!([]));

    let paul = api.sign_in(user::PAUL).await;
    let path = format!("/admin/connectors/{connector}");
    let allowed = paul.patch(&path, json!({ "writeTools": true })).await;
    assert_eq!(allowed.status, StatusCode::OK, "{}", allowed.text());
    let listed = links(&claire).await;
    assert_eq!(
        listed[0]["linked"]["toGrant"],
        json!([MAIL_SEND, CALENDARS_WRITE])
    );
    let accounts = PersonAccounts::new(
        api.app.services.account_links.clone(),
        tenant::ID,
        user::CLAIRE,
    );
    assert_eq!(
        accounts.access_token(MICROSOFT365, &[MAIL_SEND]).await,
        Err(AccountAccessError::NotGranted(vec![MAIL_SEND.into()]))
    );
    assert!(
        accounts
            .access_token(MICROSOFT365, &[MAIL_READ])
            .await
            .is_ok()
    );

    // Linked again, the new permissions asked and granted.
    let (url, cookie) = start(&claire, connector).await;
    assert!(
        url.contains(MAIL_SEND) && url.contains(CALENDARS_WRITE),
        "{url}"
    );
    come_back(&claire, &idp.authorize(&url), Some(&cookie)).await;
    assert_eq!(links(&claire).await[0]["linked"]["toGrant"], json!([]));
    assert!(
        accounts
            .access_token(MICROSOFT365, &[MAIL_SEND])
            .await
            .is_ok()
    );
    // Renewed with what she granted, writing included.
    let renewal = idp.renewals().last().cloned().unwrap();
    assert!(renewal.split(' ').any(|s| s == MAIL_SEND), "{renewal}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_email_leaves_once_validated_exactly_as_validated() {
    let api = TestApi::start_with(&SSO_DEPLOYMENT).await;
    let idp = MockIdentityProvider::start().await;
    let graph = FakeGraph::start().await;
    let connector = insert_microsoft365_writing(&api, &idp, &graph.url).await;
    let claire = api.sign_in(user::CLAIRE).await;
    idp.sign_in_as(microsoft_person("objet-claire", &email(user::CLAIRE)));
    assert_eq!(location(&link(&claire, &idp, connector).await), LINKED);
    let input = json!({
        "to": ["paul@exemple.fr"],
        "subject": "Planning de novembre",
        "body": "Bonjour Paul,\n\nLe planning est prêt.",
    });
    api.script(vec![
        ScriptedStep::calls("", vec![call("m365_mail_send", input.clone())]),
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
    assert_eq!(request["toolName"], "m365_mail_send");
    assert_eq!(request["risk"], "external");
    // The exact parameters executed, its copy list empty.
    let mut exact = input.clone();
    exact["cc"] = json!([]);
    assert_eq!(request["payload"], exact);
    let summary = request["actionSummary"].as_str().unwrap();
    assert!(summary.contains("« Planning de novembre »") && summary.contains("paul@exemple.fr"));
    assert!(graph.requests().is_empty(), "nothing leaves before");

    let decision = format!("/approvals/{}/decision", request["id"].as_str().unwrap());
    let decided = claire.post(&decision, json!({ "approved": true })).await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    api.idle().await;
    assert_eq!(api.model.as_ref().unwrap().remaining(), 0, "steps left");
    let requests = graph.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/me/sendMail");
    assert_eq!(requests[0].token, "acces-sujet-objet-claire-1");
    let message = &requests[0].body.as_ref().unwrap()["message"];
    assert_eq!(message["subject"], input["subject"]);
    assert_eq!(message["body"]["content"], input["body"]);
    assert_eq!(
        message["toRecipients"][0]["emailAddress"]["address"],
        "paul@exemple.fr"
    );
}
