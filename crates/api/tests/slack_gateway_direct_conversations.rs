//! Whose direct conversation a message comes from (ADR-131): its signature
//! proves Slack sent it, not that the conversation it names is the
//! writer's. A linked person's message from another's direct conversation
//! starts nothing and says nothing there; a stranger's is offered no link
//! there; when Slack does not say whose a conversation is (no `im:read`),
//! nothing is started nor said, until it says so again.

mod support;

use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::FakeSlack;
use mimorya_core::demo::user;
use support::gateways::{declare_slack, direct_message, dm_of, send};
use support::slack_conversations::{
    MARC_SLACK, answer, event_id, linked, settle, slack_runs, write, written,
};
use support::sso::SSO_DEPLOYMENT;
use support::{TestApi, email};

const LEA_SLACK: &str = "U0LEAFONT";

/// What `person` writes at `ts` as Slack sends it, but from the direct
/// conversation of `other`.
fn written_in(person: &str, other: &str, text: &str, ts: &str) -> String {
    written(person, text, ts, None).replace(&dm_of(person), &dm_of(other))
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

    // As Marc, from Léa's direct conversation: nothing there.
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
async fn a_stranger_is_offered_no_link_in_another_s_conversation() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let paul = api.sign_in(user::PAUL).await;
    let gateway = declare_slack(&paul).await;
    slack.add_person(LEA_SLACK, &email(user::LEA));

    let forged = direct_message(&event_id(), LEA_SLACK, "Bonjour")
        .replace(&dm_of(LEA_SLACK), &dm_of(MARC_SLACK));
    send(&api, gateway, &forged).await;
    settle(&api).await;
    assert!(slack.posts().is_empty(), "{:?}", slack.posts());
    // From her own: the link, there.
    send(
        &api,
        gateway,
        &direct_message(&event_id(), LEA_SLACK, "Bonjour"),
    )
    .await;
    settle(&api).await;
    let posts = slack.posts();
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert_eq!(posts[0].channel, dm_of(LEA_SLACK));
    assert!(posts[0].text.contains("gateway-link#"), "{}", posts[0].text);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn nothing_is_started_where_slack_does_not_say_whose_it_is() {
    let slack = FakeSlack::start().await;
    let api = TestApi::start_with_slack(&SSO_DEPLOYMENT, &slack.url).await;
    let (paul, marc) = (api.sign_in(user::PAUL).await, api.sign_in(user::MARC).await);
    let address = email(user::MARC);
    let gateway = linked(&api, &slack, &paul, &marc, (MARC_SLACK, &address)).await;
    let before = slack.posts().len();

    // Without im:read, Slack refuses to say: nothing started, nor said.
    slack.refuse_conversations_with(Some("missing_scope"));
    let first = written(MARC_SLACK, "Mes interventions", "1700000200.000100", None);
    write(&api, gateway, &first).await;
    slack.refuse_conversations_with(None);
    assert!(slack_runs(&api).await.is_empty());
    assert_eq!(slack.posts().len(), before, "nothing said");

    // Said again: answered.
    api.script(vec![ScriptedStep::text("Réponse.")]);
    let again = written(MARC_SLACK, "Mes interventions", "1700000300.000100", None);
    write(&api, gateway, &again).await;
    assert_eq!(slack_runs(&api).await.len(), 1);
}
