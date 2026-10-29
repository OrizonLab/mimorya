//! The validations given in Slack (ADR-129): a run started there that
//! pauses for its author's own validation shows, in its thread, each action
//! awaiting it with its exact parameters, whole, and a « Valider » and a
//! « Refuser » button; Slack sends the application which button a person
//! pressed (`block_actions`), signed as its events. Only an action that
//! changes nothing outside the company and can be undone, whose exact
//! parameters fit whole in a message, is validated there
//! (`validated_in_slack`); any other, in the application. Only the
//! application's buttons are read: their approval, the person who pressed,
//! where the message is. The message, once an action is decided, shows what
//! became of each of its actions in place of their buttons.

use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;
use uuid::Uuid;

use super::events::{is_person, is_workspace};
use super::mrkdwn::escaped;
use crate::autonomy::ToolRisk;
use crate::gateways::bounded;

/// The buttons, as the application names them.
pub const APPROVE_ACTION: &str = "approval_approve";
pub const REJECT_ACTION: &str = "approval_reject";
/// How an action's buttons are named in their message, before its approval.
const BLOCK_PREFIX: &str = "approval:";
/// The most of an action's description shown in Slack: escaped, it stays
/// within the 3 000 characters of a section.
pub const MAX_ACTION_CHARS: usize = 500;
/// The most of an action's exact parameters a message shows: beyond, it is
/// validated in the application, which shows them whole.
pub const MAX_PARAMETERS_CHARS: usize = 2_500;
/// The most blocks a message holds (Slack refuses more than 50).
const MAX_BLOCKS: usize = 50;

/// Any conversation the application writes in.
static CHANNEL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[CDG][A-Z0-9]{2,30}$").expect("valid regex"));
/// A message's timestamp, which names it in its conversation.
static TIMESTAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{1,12}\.[0-9]{1,9}$").expect("valid regex"));

/// A person's choice on an action awaiting their validation, as Slack sent
/// it: which button, on which message.
#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalChoice {
    /// The workspace it was pressed in.
    pub team_id: String,
    /// Who pressed it (`U…`).
    pub person: String,
    /// The conversation of the message.
    pub channel: String,
    /// The message the button is in.
    pub message_ts: String,
    /// The thread it is in, if any.
    pub thread_ts: Option<String>,
    pub approval_id: Uuid,
    pub approved: bool,
    /// The message's blocks, as Slack holds them: rewritten once decided.
    pub blocks: Vec<Value>,
}

/// An action awaiting its author's validation, shown in Slack.
#[derive(Debug, Clone, Copy)]
pub struct ShownAction<'a> {
    pub approval_id: Uuid,
    /// What the agent says it does.
    pub summary: &'a str,
    /// Its risk, as stored (`write`…).
    pub risk: &'a str,
    /// Its exact parameters, as they will be carried out.
    pub parameters: &'a Value,
}

/// The choice `payload` (the JSON Slack posts, form-encoded, as `payload`)
/// tells, if it is a press of one of the application's buttons.
pub fn approval_choice(payload: &str) -> Option<ApprovalChoice> {
    let payload: Value = serde_json::from_str(payload).ok()?;
    if payload.get("type")?.as_str()? != "block_actions" {
        return None;
    }
    let text = |value: &Value, key: &str| value.get(key)?.as_str().map(str::to_owned);
    let team_id = text(payload.get("team")?, "id")?;
    let person = text(payload.get("user")?, "id")?;
    let container = payload.get("container")?;
    let channel = text(container, "channel_id")?;
    let message_ts = text(container, "message_ts")?;
    let thread_ts = text(container, "thread_ts");
    // One button pressed at a time.
    let [action] = payload.get("actions")?.as_array()?.as_slice() else {
        return None;
    };
    let approved = match action.get("action_id")?.as_str()? {
        APPROVE_ACTION => true,
        REJECT_ACTION => false,
        _ => return None,
    };
    let approval_id = Uuid::parse_str(action.get("value")?.as_str()?).ok()?;
    let blocks = payload
        .get("message")
        .and_then(|m| m.get("blocks"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let named = is_workspace(&team_id)
        && is_person(&person)
        && CHANNEL.is_match(&channel)
        && TIMESTAMP.is_match(&message_ts)
        && thread_ts.as_deref().is_none_or(|ts| TIMESTAMP.is_match(ts))
        && blocks.len() <= MAX_BLOCKS;
    named.then_some(ApprovalChoice {
        team_id,
        person,
        channel,
        message_ts,
        thread_ts,
        approval_id,
        approved,
        blocks,
    })
}

/// Whether an action of `risk` with these exact `parameters` may be
/// validated in Slack: never one that sends something outside the company
/// or cannot be undone (validated in the application, ADR-119, ADR-123),
/// nor one whose exact parameters do not fit whole in a message.
pub fn validated_in_slack(risk: &str, parameters: &Value) -> bool {
    let undone = matches!(
        ToolRisk::parse(risk),
        Some(ToolRisk::Read | ToolRisk::Draft | ToolRisk::Write)
    );
    undone && exact_parameters(parameters).chars().count() <= MAX_PARAMETERS_CHARS
}

/// The exact parameters, as Slack shows them: indented JSON.
fn exact_parameters(parameters: &Value) -> String {
    serde_json::to_string_pretty(parameters).unwrap_or_else(|_| parameters.to_string())
}

fn risk_label(risk: &str) -> &'static str {
    match ToolRisk::parse(risk) {
        Some(ToolRisk::Read) => "Lecture",
        Some(ToolRisk::Draft) => "Brouillon",
        Some(ToolRisk::Write) => "Modification",
        Some(ToolRisk::External) => "Impact externe",
        Some(ToolRisk::Irreversible) | None => "Irréversible",
    }
}

/// The blocks of a pause's message: `intro`, in Slack's markup, then each
/// action: its description (escaped, bounded) and its risk, its exact
/// parameters whole, as literal text, and its two buttons.
pub fn approval_blocks(intro: &str, actions: &[ShownAction]) -> Vec<Value> {
    let mut blocks = vec![section(intro)];
    for action in actions {
        let (summary, _) = bounded(action.summary.trim(), MAX_ACTION_CHARS);
        blocks.push(section(&format!(
            "*Action à valider :* {}\n*Risque :* {}. Paramètres exacts qui seront exécutés :",
            escaped(&summary),
            risk_label(action.risk)
        )));
        blocks.push(json!({
            "type": "rich_text",
            "elements": [{
                "type": "rich_text_preformatted",
                "elements": [{ "type": "text", "text": exact_parameters(action.parameters) }],
            }],
        }));
        let value = action.approval_id.to_string();
        blocks.push(json!({
            "type": "actions",
            "block_id": format!("{BLOCK_PREFIX}{}", action.approval_id),
            "elements": [
                button("Valider", APPROVE_ACTION, &value, "primary"),
                button("Refuser", REJECT_ACTION, &value, "danger"),
            ],
        }));
    }
    blocks
}

/// The approvals whose buttons are in `blocks`.
pub fn buttons_in(blocks: &[Value]) -> Vec<Uuid> {
    (blocks.iter())
        .filter_map(|b| b.get("block_id")?.as_str()?.strip_prefix(BLOCK_PREFIX))
        .filter_map(|id| Uuid::parse_str(id).ok())
        .collect()
}

/// `blocks` with the buttons of each action of `outcomes` replaced by what
/// became of it (in Slack's markup); the others left as they are.
pub fn decided_blocks(blocks: &[Value], outcomes: &[(Uuid, String)]) -> Vec<Value> {
    (blocks.iter())
        .map(|block| {
            let named = block.get("block_id").and_then(Value::as_str);
            let outcome = outcomes
                .iter()
                .find(|(id, _)| named == Some(format!("{BLOCK_PREFIX}{id}").as_str()));
            match outcome {
                Some((id, text)) => json!({
                    "type": "context",
                    "block_id": format!("{BLOCK_PREFIX}{id}"),
                    "elements": [{ "type": "mrkdwn", "text": text }],
                }),
                None => block.clone(),
            }
        })
        .collect()
}

fn section(text: &str) -> Value {
    json!({ "type": "section", "text": { "type": "mrkdwn", "text": text } })
}

fn button(label: &str, action_id: &str, value: &str, style: &str) -> Value {
    json!({
        "type": "button",
        "text": { "type": "plain_text", "text": label },
        "action_id": action_id,
        "value": value,
        "style": style,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPROVAL: &str = "7b0e4c4e-55d0-4f55-9a43-2d1c1c7d2f10";

    fn id() -> Uuid {
        Uuid::parse_str(APPROVAL).unwrap()
    }

    fn shown<'a>(summary: &'a str, risk: &'a str, parameters: &'a Value) -> ShownAction<'a> {
        ShownAction {
            approval_id: id(),
            summary,
            risk,
            parameters,
        }
    }

    fn pressed(action_id: &str, value: &str) -> Value {
        let parameters = json!({ "proposalId": "p-1" });
        json!({
            "type": "block_actions",
            "team": { "id": "T0NEOMAINT" },
            "user": { "id": "U0MARCLEF" },
            "container": {
                "type": "message",
                "channel_id": "D0MARC",
                "message_ts": "1700000001.000200",
                "thread_ts": "1700000000.000100",
            },
            "message": { "blocks": approval_blocks("Pause", &[shown("Appliquer", "write", &parameters)]) },
            "actions": [{ "action_id": action_id, "value": value }],
        })
    }

    #[test]
    fn a_press_of_the_application_s_buttons_is_read() {
        let choice = approval_choice(&pressed(APPROVE_ACTION, APPROVAL).to_string()).unwrap();
        assert_eq!(choice.person, "U0MARCLEF");
        assert_eq!(choice.channel, "D0MARC");
        assert_eq!(choice.message_ts, "1700000001.000200");
        assert_eq!(choice.thread_ts.as_deref(), Some("1700000000.000100"));
        assert_eq!(choice.approval_id, id());
        assert!(choice.approved);
        assert_eq!(choice.blocks.len(), 4);
        let refused = approval_choice(&pressed(REJECT_ACTION, APPROVAL).to_string()).unwrap();
        assert!(!refused.approved);
    }

    #[test]
    fn anything_else_is_not() {
        assert_eq!(approval_choice("pas du JSON"), None);
        assert_eq!(
            approval_choice(&pressed("autre", APPROVAL).to_string()),
            None
        );
        assert_eq!(
            approval_choice(&pressed(APPROVE_ACTION, "pas-un-id").to_string()),
            None
        );
        let mut other = pressed(APPROVE_ACTION, APPROVAL);
        other["type"] = json!("view_submission");
        assert_eq!(approval_choice(&other.to_string()), None);
        let mut two = pressed(APPROVE_ACTION, APPROVAL);
        let action = two["actions"][0].clone();
        two["actions"] = json!([action.clone(), action]);
        assert_eq!(approval_choice(&two.to_string()), None);
        let mut channel = pressed(APPROVE_ACTION, APPROVAL);
        channel["container"]["channel_id"] = json!("<!here>");
        assert_eq!(approval_choice(&channel.to_string()), None);
    }

    #[test]
    fn only_an_action_that_can_be_undone_with_parameters_that_fit_is_validated_in_slack() {
        let small = json!({ "proposalId": "p-1" });
        assert!(validated_in_slack("read", &small));
        assert!(validated_in_slack("draft", &small));
        assert!(validated_in_slack("write", &small));
        assert!(!validated_in_slack("external", &small));
        assert!(!validated_in_slack("irreversible", &small));
        assert!(!validated_in_slack("inconnu", &small));
        let large = json!({ "body": "x".repeat(MAX_PARAMETERS_CHARS) });
        assert!(!validated_in_slack("write", &large));
    }

    #[test]
    fn an_action_shows_its_risk_and_its_exact_parameters_whole() {
        let parameters = json!({ "to": "<!channel> & co", "body": "Bonjour" });
        let blocks = approval_blocks(
            "Une action attend",
            &[shown("Écrire à <!channel>", "write", &parameters)],
        );
        let described = blocks[1]["text"]["text"].as_str().unwrap();
        assert!(
            described.starts_with("*Action à valider :* Écrire à &lt;!channel&gt;"),
            "{described}"
        );
        assert!(described.contains("*Risque :* Modification"), "{described}");
        // Literal text, never read as Slack's markup: shown as is.
        let exact = blocks[2]["elements"][0]["elements"][0]["text"]
            .as_str()
            .unwrap();
        assert_eq!(exact, serde_json::to_string_pretty(&parameters).unwrap());
        assert_eq!(blocks[3]["block_id"], format!("approval:{APPROVAL}"));
        assert_eq!(blocks[3]["elements"][0]["action_id"], APPROVE_ACTION);
        assert_eq!(blocks[3]["elements"][0]["value"], APPROVAL);
        assert_eq!(blocks[3]["elements"][1]["action_id"], REJECT_ACTION);
        let long = "x".repeat(MAX_ACTION_CHARS * 2);
        let bounded = approval_blocks("Pause", &[shown(&long, "write", &parameters)]);
        let summary = bounded[1]["text"]["text"].as_str().unwrap();
        assert!(
            summary.chars().count() < MAX_ACTION_CHARS + 120,
            "{summary}"
        );
    }

    #[test]
    fn each_decided_action_shows_its_outcome_in_place_of_its_buttons() {
        let parameters = json!({});
        let other = Uuid::new_v4();
        let mut second = shown("Autre", "write", &parameters);
        second.approval_id = other;
        let blocks = approval_blocks("Pause", &[shown("Appliquer", "write", &parameters), second]);
        assert_eq!(buttons_in(&blocks), [id(), other]);
        let decided = decided_blocks(&blocks, &[(id(), "Validée".into())]);
        assert_eq!(decided.len(), 7);
        assert_eq!(decided[3]["type"], "context");
        assert_eq!(decided[3]["elements"][0]["text"], "Validée");
        assert_eq!(decided[6]["type"], "actions");
        let both = decided_blocks(
            &decided,
            &[(id(), "Validée".into()), (other, "Refusée".into())],
        );
        assert_eq!(both[6]["elements"][0]["text"], "Refusée");
        assert_eq!(buttons_in(&both), [id(), other], "named alike once decided");
    }
}
