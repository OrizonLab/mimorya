//! A person links their Slack identity to their account (ADR-125): the link
//! the application sent them in Slack, opened in their own session, is
//! confirmed once Slack says that Slack user is of the company's workspace
//! and has the account's own address. A link opened by someone else, of a
//! Slack user of another workspace, without an address or deleted, expired
//! or of another company links nothing; refusals are audited, never an
//! address. Linking takes using the agent, an enabled gateway, and Slack
//! answering the application. A new link replaces the former one, and this
//! identity's link to anyone else. The person lists and removes it; a
//! deactivated account loses it.

mod support;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::AuthIdentity;
use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::gateways::{
    ACCOUNT, LINK, PREVIEW, declare_slack, direct_message, gateway_view, offered_code, send,
};
use support::manager_approvals::without;
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::settle;
use support::sso::SSO_DEPLOYMENT;
use support::{Client, TestApi, companies, email};
use uuid::Uuid;

const LEA_SLACK: &str = "U0LEAFONT";

/// The link the application sends `person` when they write to it.
async fn link_for(api: &TestApi, slack: &FakeSlack, gateway: Uuid, person: &str) -> String {
    let event_id = format!("Ev0{}", Uuid::new_v4().simple());
    let sent = send(api, gateway, &direct_message(&event_id, person, "Bonjour")).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.text());
    api.idle().await;
    offered_code(slack)
}

async fn confirm(client: &Client<'_>, code: &str) -> support::TestResponse {
    client.post(LINK, json!({ "code": code })).await
}

/// The person's gateways, as listed to them.
async fn own_gateway(client: &Client<'_>, gateway: Uuid) -> Value {
    let listed = client.get(ACCOUNT).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.text());
    listed.json()["gateways"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["gatewayId"] == gateway.to_string())
        .cloned()
        .unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_person_links_their_slack_identity_from_their_own_session() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let lea = api.sign_in(user::LEA).await;
    let listed = own_gateway(&lea, gateway).await;
    assert_eq!(listed["type"], "slack");
    assert_eq!(listed["available"], true);
    assert_eq!(listed["linked"], Value::Null);

    let code = link_for(&api, &slack, gateway, LEA_SLACK).await;
    let shown = lea.post(PREVIEW, json!({ "code": code })).await;
    assert_eq!(shown.status, StatusCode::OK, "{}", shown.text());
    let shown = shown.json();
    assert_eq!(shown["gatewayId"], gateway.to_string());
    assert_eq!(shown["workspaceName"], "Néo Maintenance");
    assert_eq!(shown["externalId"], LEA_SLACK);

    let since = latest_entry(&api).await;
    let linked = confirm(&lea, &code).await;
    assert_eq!(linked.status, StatusCode::OK, "{}", linked.text());
    let listed = own_gateway(&lea, gateway).await;
    assert_eq!(listed["linked"]["externalId"], LEA_SLACK);
    assert_eq!(listed["linked"]["address"], email(user::LEA));
    // Counted for the administrators, never named.
    let view = gateway_view(&paul, gateway).await;
    assert_eq!(view["identities"], 1);
    assert!(!view.to_string().contains(LEA_SLACK));
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.identity_linked"), "{audited}");
    assert!(!audited.contains(&email(user::LEA)), "{audited}");

    // Linked, she is offered no link any more: her message is answered in a
    // conversation of hers (ADR-127).
    let before = slack.posts().len();
    api.script(vec![ScriptedStep::text("Bonjour Léa.")]);
    let sent = send(
        &api,
        gateway,
        &direct_message("Ev0LINKED001", LEA_SLACK, "Re"),
    )
    .await;
    assert_eq!(sent.status, StatusCode::OK);
    settle(&api).await;
    assert_eq!(slack.posts().len(), before);

    // She removes it.
    let removed = lea.delete(&format!("{ACCOUNT}/{gateway}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    assert_eq!(own_gateway(&lea, gateway).await["linked"], Value::Null);
    let again = lea.delete(&format!("{ACCOUNT}/{gateway}")).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
    assert!(
        entries_since(&api, since)
            .await
            .contains("gateway.identity_removed")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_slack_s_word_for_the_account_s_own_address_links() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let since = latest_entry(&api).await;

    // Léa's link, opened by Paul: her address is not his account's.
    let code = link_for(&api, &slack, gateway, LEA_SLACK).await;
    let refused = confirm(&paul, &code).await;
    assert_eq!(
        refused.status,
        StatusCode::BAD_REQUEST,
        "{}",
        refused.text()
    );
    assert!(
        refused
            .error_message()
            .starts_with("L’adresse de ce compte Slack n’est pas celle")
    );
    assert!(!refused.text().contains(&email(user::LEA)));
    assert_eq!(own_gateway(&paul, gateway).await["linked"], Value::Null);

    // Of another workspace, without an address, deleted.
    let lea = api.sign_in(user::LEA).await;
    for (person, refusal) in [
        ("U0ETRANGER", "n’est pas de l’espace de travail"),
        ("U0SANSADR", "users:read.email"),
        ("U0PARTI", "désactivé"),
    ] {
        match person {
            "U0ETRANGER" => slack.add_foreigner(person, &email(user::LEA)),
            "U0SANSADR" => slack.add_person_as(person, None, false),
            _ => slack.add_person_as(person, Some(&email(user::LEA)), true),
        }
        let code = link_for(&api, &slack, gateway, person).await;
        let refused = confirm(&lea, &code).await;
        assert_eq!(refused.status, StatusCode::BAD_REQUEST, "{person}");
        assert!(
            refused.error_message().contains(refusal),
            "{}",
            refused.text()
        );
    }
    // Unknown to Slack.
    let code = link_for(&api, &slack, gateway, "U0INCONNU").await;
    let refused = confirm(&lea, &code).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(own_gateway(&lea, gateway).await["linked"], Value::Null);
    let audited = entries_since(&api, since).await;
    for reason in ["other_address", "other_workspace", "no_address", "deleted"] {
        assert!(audited.contains(reason), "{reason}: {audited}");
    }
    assert!(!audited.contains(&email(user::LEA)), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_link_is_valid_fifteen_minutes_in_its_own_company() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let code = link_for(&api, &slack, gateway, LEA_SLACK).await;
    let lea = api.sign_in(user::LEA).await;

    // Altered, it opens nothing.
    let mut altered = code.clone();
    altered.pop();
    altered.push(if code.ends_with('A') { 'B' } else { 'A' });
    for broken in [altered.as_str(), "pas-un-lien", ""] {
        let refused = confirm(&lea, broken).await;
        assert!(
            matches!(
                refused.status,
                StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST
            ),
            "{broken}: {}",
            refused.text()
        );
        let shown = lea.post(PREVIEW, json!({ "code": broken })).await;
        assert_eq!(shown.status.as_u16() / 100, 4, "{broken}");
    }

    // Sixteen minutes later.
    let gateways = &api.app.services.gateways;
    let access = |tenant_id, user_id| {
        api.app.services.access.context(AuthIdentity {
            tenant_id,
            user_id,
            session_id: Uuid::nil(),
            must_change_password: false,
        })
    };
    let lea_access = access(tenant::ID, user::LEA).await.unwrap();
    let later = Utc::now() + Duration::minutes(16);
    let expired = gateways.preview_link(&lea_access, &code, later).await;
    let expired = expired.err().unwrap().to_string();
    assert!(expired.starts_with("Ce lien a expiré"), "{expired}");

    // In another company, as if it were none.
    let acme = companies::create(&api, "acme-slack", "jeanne.martin@acme-slack.example").await;
    // Her invitation accepted.
    as_owner(
        "update users set status = 'active' where id = $1",
        acme.admin_id,
    )
    .await;
    let jeanne = access(acme.tenant_id, acme.admin_id).await.unwrap();
    let elsewhere = gateways.preview_link(&jeanne, &code, Utc::now()).await;
    let elsewhere = elsewhere.err().unwrap().to_string();
    assert!(
        elsewhere.starts_with("Lien de liaison invalide"),
        "{elsewhere}"
    );
    let elsewhere = gateways.confirm_link(&jeanne, &code, Utc::now()).await;
    assert!(elsewhere.is_err());

    // Still Léa's to use, now.
    assert_eq!(confirm(&lea, &code).await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_link_replaces_the_former_one_and_a_deactivated_account_loses_it() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    let (lea, marc) = (api.sign_in(user::LEA).await, api.sign_in(user::MARC).await);

    // Marc links a Slack user of his address; a second link was sent for it
    // meanwhile.
    slack.add_person("U0PARTAGE", &email(user::MARC));
    let code = link_for(&api, &slack, gateway, "U0PARTAGE").await;
    let later = link_for(&api, &slack, gateway, "U0PARTAGE").await;
    assert_eq!(confirm(&marc, &code).await.status, StatusCode::OK);
    // Slack gives that user Léa's address since: her link takes it from him.
    slack.add_person("U0PARTAGE", &email(user::LEA));
    let since = latest_entry(&api).await;
    assert_eq!(confirm(&lea, &later).await.status, StatusCode::OK);
    assert_eq!(
        own_gateway(&lea, gateway).await["linked"]["externalId"],
        "U0PARTAGE"
    );
    assert_eq!(own_gateway(&marc, gateway).await["linked"], Value::Null);
    let audited = entries_since(&api, since).await;
    assert!(
        audited.contains("\"replacedOtherPerson\":true"),
        "{audited}"
    );

    // Her own new Slack user replaces her former one.
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let code = link_for(&api, &slack, gateway, LEA_SLACK).await;
    assert_eq!(confirm(&lea, &code).await.status, StatusCode::OK);
    assert_eq!(
        own_gateway(&lea, gateway).await["linked"]["externalId"],
        LEA_SLACK
    );
    assert_eq!(gateway_view(&paul, gateway).await["identities"], 1);
    assert!(
        entries_since(&api, since)
            .await
            .contains("\"replacedOwn\":true")
    );

    // Deactivated, she loses it; reactivated, she links it again herself.
    let path = format!("/admin/users/{}", user::LEA);
    let disabled = paul.patch(&path, json!({ "status": "disabled" })).await;
    assert_eq!(disabled.status, StatusCode::OK, "{}", disabled.text());
    assert_eq!(gateway_view(&paul, gateway).await["identities"], 0);
    let enabled = paul.patch(&path, json!({ "status": "active" })).await;
    assert_eq!(enabled.status, StatusCode::OK, "{}", enabled.text());
    let lea = api.sign_in(user::LEA).await;
    assert_eq!(own_gateway(&lea, gateway).await["linked"], Value::Null);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn linking_takes_the_agent_an_enabled_gateway_and_slack_s_answer() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));
    let code = link_for(&api, &slack, gateway, LEA_SLACK).await;
    let lea = api.sign_in(user::LEA).await;

    // Without using the agent: shown unavailable, nothing linked.
    let policy = without(&paul, user::LEA, "agent:use").await;
    assert_eq!(own_gateway(&lea, gateway).await["available"], false);
    let refused = confirm(&lea, &code).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.text());
    assert_eq!(own_gateway(&lea, gateway).await["linked"], Value::Null);
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);
    assert_eq!(own_gateway(&lea, gateway).await["available"], true);

    // A paused gateway shows and links nothing.
    let path = format!("/admin/gateways/{gateway}");
    let paused = paul.patch(&path, json!({ "enabled": false })).await;
    assert_eq!(paused.status, StatusCode::OK);
    assert_eq!(own_gateway(&lea, gateway).await["available"], false);
    let shown = lea.post(PREVIEW, json!({ "code": code })).await;
    assert_eq!(shown.status, StatusCode::CONFLICT, "{}", shown.text());
    let refused = confirm(&lea, &code).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    assert!(
        refused.error_message().contains("désactivée"),
        "{}",
        refused.text()
    );
    assert_eq!(own_gateway(&lea, gateway).await["linked"], Value::Null);
    let resumed = paul.patch(&path, json!({ "enabled": true })).await;
    assert_eq!(resumed.status, StatusCode::OK);

    // Slack refuses the application, its permissions or its token: her
    // administrator's to fix.
    slack.refuse_with(Some("missing_scope"));
    let refused = confirm(&lea, &code).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        refused
            .error_message()
            .contains("prévenez votre administrateur"),
        "{}",
        refused.text()
    );
    slack.refuse_with(None);
    slack.revoke();
    let refused = confirm(&lea, &code).await;
    assert_eq!(refused.status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        refused
            .error_message()
            .contains("prévenez votre administrateur"),
        "{}",
        refused.text()
    );
    assert_eq!(own_gateway(&lea, gateway).await["linked"], Value::Null);
}
