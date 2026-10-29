//! Whose direct conversation a message comes from (ADR-131): its signature
//! proves Slack sent it, not that the conversation it names is the
//! writer's. A linked person's message from another's direct conversation
//! starts nothing and says nothing there, audited for the administrators; a
//! stranger is offered a link in their own conversation only; when Slack
//! does not say whose a conversation is (no `im:read`), nothing is started,
//! nor decided, and the writer is told so in their own conversation,
//! never the one the event names.

mod support;

use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::{FakeSlack, SlackPost};
use mimorya_core::demo::user;
use support::gateways::{declare_slack, direct_message, dm_of, send};
use support::manager_approvals::listed;
use support::refusals::{entries_since, latest_entry};
use support::slack_conversations::{
    MARC_SLACK, answer, event_id, linked, settle, slack_runs, write, written,
};
use support::slack_validations::{paused, press, pressed_by};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

const LEA_SLACK: &str = "U0LEAFONT";

/// What `person` writes at `ts` as Slack sends it, but from the direct
/// conversation of `other`.
fn written_in(person: &str, other: &str, text: &str, ts: &str) -> String {
    written(person, text, ts, None).replace(&dm_of(person), &dm_of(other))
}

/// What the application wrote in the direct conversation of `person`
/// since `before` messages.
fn new_in(slack: &FakeSlack, person: &str, before: usize) -> Vec<SlackPost> {
    let channel = dm_of(person);
    (slack.posts().into_iter().skip(before))
        .filter(|p| p.channel == channel)
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_message_from_another_s_direct_conversation_starts_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let before = slack.posts().len();

    // As Marc, from Léa's direct conversation: nothing, audited.
    let since = latest_entry(&api).await;
    api.script(vec![ScriptedStep::text("Réponse pour Marc.")]);
    let forged = written_in(
        MARC_SLACK,
        LEA_SLACK,
        "Mes interventions",
        "1700000200.000100",
    );
    write(&api, gateway, &forged).await;
    assert!(slack_runs(&api).await.is_empty());
    assert_eq!(answer(&api).await, 0);
    assert_eq!(slack.posts().len(), before, "nothing said");
    let audited = entries_since(&api, since).await;
    let refused: serde_json::Value = serde_json::from_str(&audited).unwrap();
    let refused = &refused[0];
    assert_eq!(
        refused["action"], "gateway.conversation_refused",
        "{audited}"
    );
    assert_eq!(refused["outcome"], "denied");
    assert_eq!(refused["resource_type"], "gateway");
    assert_eq!(refused["resource_id"], gateway.to_string());
    assert_eq!(
        refused["metadata"],
        serde_json::json!({ "externalId": MARC_SLACK })
    );
    assert!(!audited.contains("Mes interventions"), "{audited}");
    // A conversation the application cannot see: nothing either, audited.
    slack.refuse_conversations_with(Some("channel_not_found"));
    let unseen = written_in(MARC_SLACK, LEA_SLACK, "Encore", "1700000250.000100");
    write(&api, gateway, &unseen).await;
    slack.refuse_conversations_with(None);
    assert_eq!(slack.posts().len(), before, "nothing said");
    let audited = entries_since(&api, since).await;
    assert_eq!(audited.matches("gateway.conversation_refused").count(), 2);

    // From his own: answered there.
    let own = written(MARC_SLACK, "Mes interventions", "1700000300.000100", None);
    write(&api, gateway, &own).await;
    assert_eq!(slack_runs(&api).await.len(), 1);
    assert_eq!(answer(&api).await, 1);
    let said = slack.posts().pop().unwrap();
    assert_eq!(said.channel, dm_of(MARC_SLACK));
    assert!(said.text.starts_with("Réponse pour Marc."), "{}", said.text);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_stranger_is_offered_a_link_in_their_own_conversation_only() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));

    // Writing from Marc's conversation: the link in hers, nothing in his.
    let forged = direct_message(&event_id(), LEA_SLACK, "Bonjour")
        .replace(&dm_of(LEA_SLACK), &dm_of(MARC_SLACK));
    send(&api, gateway, &forged).await;
    settle(&api).await;
    let posts = slack.posts();
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert_eq!(posts[0].channel, dm_of(LEA_SLACK));
    assert!(posts[0].text.contains("gateway-link#"), "{}", posts[0].text);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn where_slack_does_not_say_whose_it_is_the_writer_is_told_in_their_own() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;

    // Without im:read, Slack refuses to say: nothing started; Marc told,
    // in his own conversation, whatever the event named.
    slack.refuse_conversations_with(Some("missing_scope"));
    let before = slack.posts().len();
    let forged = written_in(
        MARC_SLACK,
        LEA_SLACK,
        "Mes interventions",
        "1700000200.000100",
    );
    write(&api, gateway, &forged).await;
    assert!(new_in(&slack, LEA_SLACK, before).is_empty());
    let told = new_in(&slack, MARC_SLACK, before);
    assert_eq!(told.len(), 1, "{told:?}");
    assert!(told[0].text.contains("im:read"), "{}", told[0].text);
    assert!(
        told[0].text.contains("rien n’a été lancé"),
        "{}",
        told[0].text
    );
    assert_eq!(told[0].thread_ts, None);
    // Slack unreachable alike: told to try again.
    slack.refuse_conversations_with(Some("ratelimited"));
    let own = written(MARC_SLACK, "Mes interventions", "1700000250.000100", None);
    write(&api, gateway, &own).await;
    let told = slack.posts().pop().unwrap();
    assert!(told.text.starts_with("Slack ne dit pas"), "{}", told.text);
    assert_eq!(told.channel, dm_of(MARC_SLACK));
    assert_eq!(told.thread_ts, None);
    slack.refuse_conversations_with(None);
    assert!(slack_runs(&api).await.is_empty());

    // Said again: answered.
    api.script(vec![ScriptedStep::text("Réponse.")]);
    let again = written(MARC_SLACK, "Mes interventions", "1700000300.000100", None);
    write(&api, gateway, &again).await;
    assert_eq!(slack_runs(&api).await.len(), 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_button_pressed_where_slack_does_not_say_whose_it_is_decides_nothing() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let (run, pause) = paused(&api, &slack, (gateway, MARC_SLACK), "Appliqué.").await;

    slack.refuse_conversations_with(Some("missing_scope"));
    press(&api, gateway, &pressed_by(MARC_SLACK, &pause, true), false).await;
    settle(&api).await;
    slack.refuse_conversations_with(None);
    let told = slack.posts().pop().unwrap();
    assert_eq!(told.channel, dm_of(MARC_SLACK));
    assert_eq!(told.thread_ts, None);
    assert!(told.text.contains("rien n’a été décidé"), "{}", told.text);
    assert_eq!(listed(&marc, &run).await.unwrap()["status"], "pending");
}
