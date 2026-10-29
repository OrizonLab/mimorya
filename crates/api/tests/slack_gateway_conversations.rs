//! A linked person talks with the agent in Slack (ADR-127): their message to
//! the application starts a run as them, in a new conversation of theirs
//! held in the thread it opens; their replies in the thread follow it; a
//! new message opens another. The run's end is written in the thread, in
//! Slack's markup, with the link to its conversation; a pause for
//! validation says where to give it, and the answer follows once given.
//! Slack is asked at each message who the Slack user is: no longer the
//! person, their identity is unlinked and a link offered. Nothing is
//! started for someone who cannot act, nor in a thread held for another
//! account or opened meanwhile, nor while the service stops, and why is
//! said in the thread.

mod support;

use axum::http::StatusCode;
use mimorya_agent::testing::ScriptedStep;
use mimorya_api::services::AuthIdentity;
use mimorya_api::services::runs::SlackThread;
use mimorya_core::DomainError;
use mimorya_core::demo::{tenant, user};
use serde_json::{Value, json};
use support::gateways::{ACCOUNT, declare_slack};
use support::manager_approvals::{listed, without};
use support::planning::{apply_proposal, propose};
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::{
    MARC_DM, MARC_SLACK, answer, confirm, conversation_of, in_thread, in_thread_of, link_code,
    linked, replies_of, settle, slack_runs, write, written,
};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, WEB_ORIGIN, email};

use mimorya_connectors::testing::FakeSlack;

const FIRST: &str = "1700000200.000100";

/// What the model was last asked.
fn last_asked(api: &TestApi) -> String {
    let requests = api.model.as_ref().unwrap().requests();
    format!("{:?}", requests.last().expect("a request").messages)
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_linked_person_talks_with_the_agent_in_a_thread() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let person = (MARC_SLACK, email(user::MARC));
    let gateway = linked(&api, &slack, &paul, &marc, (person.0, &person.1)).await;
    let since = latest_entry(&api).await;

    // Outside any thread: a new conversation, held in the thread it opens.
    api.script(vec![ScriptedStep::text("**Trois** visites demain à Lyon.")]);
    let asked = "Quelles visites demain &lt;Lyon&gt; ?";
    write(&api, gateway, &written(MARC_SLACK, asked, FIRST, None)).await;
    // Read as he typed it.
    assert!(last_asked(&api).contains("Quelles visites demain <Lyon> ?"));
    let conversation = conversation_of(&api, FIRST)
        .await
        .expect("its conversation");
    let shown = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(
        shown["conversation"]["title"],
        "Quelles visites demain <Lyon> ?"
    );
    assert_eq!(
        shown["messages"][0]["metadata"]["gateway"], "slack",
        "{shown}"
    );
    let runs = slack_runs(&api).await;
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].1, "completed");

    // Its end, written in the thread, once.
    assert_eq!(answer(&api).await, 1);
    let link = format!("<{WEB_ORIGIN}/chat/{conversation}|ouvrir la conversation>");
    assert_eq!(
        in_thread(&slack, FIRST),
        [format!(
            "*Trois* visites demain à Lyon.\n\n_Dans l’application : {link}._"
        )]
    );
    assert_eq!(answer(&api).await, 0);

    // A reply in the thread follows the conversation.
    api.script(vec![ScriptedStep::text("Villeurbanne ajoutée.")]);
    let reply = written(
        MARC_SLACK,
        "Ajoute Villeurbanne",
        "1700000300.000100",
        Some(FIRST),
    );
    write(&api, gateway, &reply).await;
    assert!(
        last_asked(&api).contains("Trois"),
        "its history is replayed"
    );
    assert_eq!(conversation_of(&api, "1700000300.000100").await, None);
    let shown = marc
        .get(&format!("/conversations/{conversation}"))
        .await
        .json();
    assert_eq!(shown["messages"].as_array().unwrap().len(), 4, "{shown}");
    assert_eq!(answer(&api).await, 1);
    assert_eq!(in_thread(&slack, FIRST).len(), 2);

    // A new message opens another conversation.
    api.script(vec![ScriptedStep::text("Bonjour Marc.")]);
    let other = "1700000400.000100";
    write(&api, gateway, &written(MARC_SLACK, "Bonjour", other, None)).await;
    let second = conversation_of(&api, other)
        .await
        .expect("another conversation");
    assert_ne!(second, conversation);
    assert_eq!(answer(&api).await, 1);
    assert!(in_thread(&slack, other)[0].starts_with("Bonjour Marc."));

    // Audited as his own turns, never what he wrote.
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("agent.run.started"), "{audited}");
    assert!(audited.contains(&gateway.to_string()), "{audited}");
    assert!(!audited.contains("Villeurbanne"), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_pause_for_validation_is_said_then_the_answer_follows() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text("Planning de INT-2052 appliqué."),
    ]);
    write(
        &api,
        gateway,
        &written(MARC_SLACK, "Planifie INT-2052", FIRST, None),
    )
    .await;
    let (run, status) = slack_runs(&api).await.remove(0);
    assert_eq!(status, "awaiting_approval");
    assert_eq!(answer(&api).await, 1);
    let paused = in_thread(&slack, FIRST);
    assert!(paused[0].contains("attend votre validation"), "{paused:?}");

    // Meanwhile, the conversation is busy: said in the thread.
    let busy = written(
        MARC_SLACK,
        "Et INT-2053 ?",
        "1700000300.000100",
        Some(FIRST),
    );
    write(&api, gateway, &busy).await;
    let said = in_thread(&slack, FIRST);
    assert!(said[1].contains("déjà en cours"), "{said:?}");
    assert_eq!(slack_runs(&api).await.len(), 1);

    // Validated in the application, the run's end follows in the thread.
    let request = listed(&marc, &run.to_string()).await.expect("his request");
    let id = request["id"].as_str().unwrap();
    let decided = marc
        .post(
            &format!("/approvals/{id}/decision"),
            json!({ "approved": true }),
        )
        .await;
    assert_eq!(decided.status, StatusCode::OK, "{}", decided.text());
    settle(&api).await;
    assert_eq!(answer(&api).await, 1);
    let said = in_thread(&slack, FIRST);
    assert!(
        said[2].starts_with("Planning de INT-2052 appliqué."),
        "{said:?}"
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn when_slack_does_not_say_who_writes_nothing_is_started() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    slack.revoke();
    write(&api, gateway, &written(MARC_SLACK, "Bonjour", FIRST, None)).await;
    assert!(slack_runs(&api).await.is_empty());
    assert_eq!(conversation_of(&api, FIRST).await, None);
    // The identity stays.
    let own = marc.get(ACCOUNT).await.json();
    assert_eq!(own["gateways"][0]["linked"]["externalId"], MARC_SLACK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_slack_user_no_longer_the_person_is_unlinked_and_offered_a_link() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    // Slack gives that Slack user another address since.
    slack.add_person(MARC_SLACK, &email(user::LEA));
    let since = latest_entry(&api).await;
    write(&api, gateway, &written(MARC_SLACK, "Bonjour", FIRST, None)).await;
    settle(&api).await;
    assert!(slack_runs(&api).await.is_empty());
    let said = in_thread(&slack, FIRST);
    assert!(said[0].contains("n’est plus relié"), "{said:?}");
    let offered = slack.posts().last().unwrap().text.clone();
    assert!(offered.contains("gateway-link#"), "{offered}");
    let own = marc.get(ACCOUNT).await.json();
    assert_eq!(own["gateways"][0]["linked"], Value::Null);
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("gateway.identity_unlinked"), "{audited}");
    assert!(audited.contains("other_address"), "{audited}");
    assert!(!audited.contains(&email(user::LEA)), "{audited}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_started_for_someone_who_cannot_act_and_why_is_said() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;

    // Without using the agent.
    let policy = without(&paul, user::MARC, "agent:use").await;
    write(&api, gateway, &written(MARC_SLACK, "Bonjour", FIRST, None)).await;
    assert!(slack_runs(&api).await.is_empty());
    assert_eq!(
        conversation_of(&api, FIRST).await,
        None,
        "no conversation left"
    );
    let said = in_thread(&slack, FIRST);
    assert_eq!(said, ["Permission requise : agent:use"]);
    assert_eq!(paul.delete(&policy).await.status, StatusCode::OK);

    // With a temporary password still to replace.
    let temporary = "update users set must_change_password = $2 where id = $1";
    flag(temporary, user::MARC, true).await;
    let other = "1700000300.000100";
    write(&api, gateway, &written(MARC_SLACK, "Bonjour", other, None)).await;
    flag(temporary, user::MARC, false).await;
    assert!(slack_runs(&api).await.is_empty());
    let said = in_thread(&slack, other);
    assert!(said[0].contains("mot de passe provisoire"), "{said:?}");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_started_while_the_service_stops() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    api.app.begin_shutdown();
    write(&api, gateway, &written(MARC_SLACK, "Bonjour", FIRST, None)).await;
    assert!(slack_runs(&api).await.is_empty());
    assert_eq!(conversation_of(&api, FIRST).await, None);
    let said = in_thread(&slack, FIRST);
    assert!(said[0].starts_with("Le service redémarre"), "{said:?}");
}

/// Sets a flag of an account, as its owner would.
async fn flag(sql: &str, user_id: uuid::Uuid, value: bool) {
    let owner = sqlx::PgPool::connect(support::owner_url()).await.unwrap();
    sqlx::query(sql)
        .bind(user_id)
        .bind(value)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_thread_held_for_another_account_is_refused() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let lea = api.sign_in(user::LEA).await;
    // Marc links a Slack user; a second link was sent for it meanwhile.
    let shared = "U0PARTAGE";
    let gateway = declare_slack(&paul).await;
    slack.add_person(shared, &email(user::MARC));
    let code = link_code(&api, &slack, gateway, shared).await;
    let later = link_code(&api, &slack, gateway, shared).await;
    confirm(&marc, &code).await;
    api.script(vec![ScriptedStep::text("Réponse pour Marc.")]);
    write(
        &api,
        gateway,
        &written(shared, "Question de Marc", FIRST, None),
    )
    .await;
    // Slack gives that user Léa's address since; her link takes it.
    slack.add_person(shared, &email(user::LEA));
    confirm(&lea, &later).await;

    // Writing in Marc's thread starts nothing, says nothing of his
    // conversation, and his run's end is not written there.
    let reply = written(shared, "Et la suite ?", "1700000300.000100", Some(FIRST));
    write(&api, gateway, &reply).await;
    assert_eq!(slack_runs(&api).await.len(), 1);
    let said = in_thread_of(&slack, shared, FIRST);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("autre compte"), "{said:?}");
    assert_eq!(answer(&api).await, 0);
    assert_eq!(in_thread_of(&slack, shared, FIRST).len(), 1);
    assert_eq!(replies_of(&api, FIRST).await, [(Some("skipped".into()), 1)]);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_thread_opened_meanwhile_is_refused_as_busy() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    // Two messages of one new thread, the second started as the first
    // opened it: refused, nothing of it kept.
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id: user::MARC,
        session_id: uuid::Uuid::nil(),
        must_change_password: false,
    };
    let thread = SlackThread {
        gateway_id: gateway,
        external_id: MARC_SLACK,
        channel_id: MARC_DM,
        thread_ts: FIRST,
    };
    let runs = &api.app.services.runs;
    api.script(vec![ScriptedStep::text("Réponse.")]);
    let title = "Fil ouvert deux fois";
    runs.start_in_thread(identity, thread, title, "Bonjour")
        .await
        .unwrap();
    let refused = runs
        .start_in_thread(identity, thread, title, "Bonjour encore")
        .await
        .unwrap_err();
    let refusal = refused.downcast_ref::<DomainError>().expect("a refusal");
    assert!(
        refusal.message.contains("déjà en cours"),
        "{}",
        refusal.message
    );
    settle(&api).await;
    let listed = marc.get("/conversations").await.json();
    let titled = listed["conversations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["title"] == title)
        .count();
    assert_eq!(titled, 1, "{listed}");
}
