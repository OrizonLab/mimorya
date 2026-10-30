//! Validations given in Teams in the tests (ADR-141): a request written to
//! the bot that pauses for its author's validation, the card of its pause
//! as the test Microsoft keeps it, written again or not, and a button
//! pressed on a card as Teams sends it (a message carrying the button's
//! value, answering the card), or as a client may claim it.

use mimorya_agent::testing::ScriptedStep;
use mimorya_connectors::testing::{FakeTeams, TEAMS_APP, TeamsSent};
use mimorya_core::demo::tenant;
use serde_json::{Value, json};
use uuid::Uuid;

use super::TestApi;
use super::planning::{apply_proposal, propose};
use super::slack_conversations::settle;
use super::teams::deliver;
use super::teams_conversations::{said_to, talk, teams_runs};

/// The request `from` writes to the bot `id` to plan INT-2052, ending with
/// `end` once validated: its run and its approval.
pub async fn asked(
    api: &TestApi,
    teams: &FakeTeams,
    (id, from): (Uuid, (&str, &str)),
    end: &str,
) -> (String, Uuid) {
    api.script(vec![
        propose("INT-2052", "15:00"),
        apply_proposal(""),
        ScriptedStep::text(end),
    ]);
    talk(api, teams, id, "a-10", from, "Planifie INT-2052").await;
    let run = teams_runs(api).await.remove(0).0;
    (run.to_string(), approval_of(api, run).await)
}

/// The approval the run `run` awaits.
pub async fn approval_of(api: &TestApi, run: Uuid) -> Uuid {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let approval = sqlx::query_scalar("select id from approvals where run_id = $1")
        .bind(run)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    approval
}

/// The latest message the bots wrote to `person` with a card, as it is now.
pub fn latest_card(teams: &FakeTeams, person: &str) -> TeamsSent {
    (said_to(teams, person).into_iter())
        .rev()
        .find(|m| m.card.is_some())
        .expect("a card")
}

/// The message `id` the bots wrote, as it is now.
pub fn message(teams: &FakeTeams, id: &str) -> TeamsSent {
    (teams.sent().into_iter())
        .find(|m| m.id == id)
        .expect("the message")
}

/// The buttons `card` shows: their title and what they carry.
pub fn buttons(card: &Value) -> Vec<(String, Value)> {
    let blocks = card["body"].as_array().cloned().unwrap_or_default();
    (blocks.iter())
        .filter(|block| block["type"] == "ActionSet")
        .flat_map(|block| block["actions"].as_array().cloned().unwrap_or_default())
        .map(|button| {
            (
                button["title"].as_str().unwrap().to_owned(),
                button["data"].clone(),
            )
        })
        .collect()
}

/// Every text `card` shows, joined.
pub fn card_text(card: &Value) -> String {
    let blocks = card["body"].as_array().cloned().unwrap_or_default();
    (blocks.iter())
        .flat_map(|block| block["inlines"].as_array().cloned().unwrap_or_default())
        .map(|inline| inline["text"].as_str().unwrap_or_default().to_owned())
        .collect::<Vec<_>>()
        .join("")
}

/// What a card's button for `approval` carries: `approved` or not.
pub fn choice(approval: Uuid, approved: bool) -> Value {
    let choice = if approved { "v" } else { "r" };
    json!({ "mimorya": choice, "approval": approval.to_string() })
}

/// `from` presses, as the activity `activity_id`, a button carrying `value`
/// on the bot's card `card`, delivered to the bot `id` as Teams sends it;
/// then waits for what it decided.
pub async fn press(
    api: &TestApi,
    teams: &FakeTeams,
    (id, card): (Uuid, &str),
    (activity_id, from): (&str, (&str, &str)),
    value: Value,
) {
    let pressed = teams.press((TEAMS_APP, activity_id), from, (card, value));
    let sent = deliver(api, teams, id, &pressed).await;
    assert_eq!(sent.status.as_u16(), 200, "{}", sent.text());
    settle(api).await;
}
