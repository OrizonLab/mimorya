//! The validations given in Slack (ADR-129): a run started there that
//! pauses for its author's own validation shows, in its thread, each action
//! awaiting it with its exact parameters, whole, and a « Valider » and a
//! « Refuser » button; Slack sends the application which button a person
//! pressed (`block_actions`), signed as its events. Only an action that
//! changes nothing outside the company and can be undone, whose exact
//! parameters fit whole in a message, is validated there
//! (`validated_in_gateway`); any other, in the application. Only the
//! application's buttons are read: their approval, the person who pressed,
//! where the message is. The message, once an action is decided, shows what
//! became of each of its actions in place of their buttons, no longer as
//! one to validate (ADR-147).

use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;
use uuid::Uuid;

use super::events::{is_person, is_workspace};
use super::mrkdwn::escaped;
use crate::gateways::bounded;
use crate::gateways::validation::{
    ActionState, MAX_ACTION_CHARS, ShownAction, exact_parameters, risk_label,
};

/// The buttons, as the application names them.
pub const APPROVE_ACTION: &str = "approval_approve";
pub const REJECT_ACTION: &str = "approval_reject";
/// How an action's buttons are named in their message, before its approval.
const BLOCK_PREFIX: &str = "approval:";
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

/// The blocks of a pause's message: `intro`, in Slack's markup, then each
/// action: its description (escaped, bounded) and its risk, its exact
/// parameters whole, as literal text, and its two buttons.
pub fn approval_blocks(intro: &str, actions: &[ShownAction]) -> Vec<Value> {
    let mut blocks = vec![section(intro)];
    for action in actions {
        let (summary, _) = bounded(action.summary.trim(), MAX_ACTION_CHARS);
        blocks.push(section(&description(
            ActionState::Open,
            &escaped(&summary),
            risk_label(action.risk),
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
/// became of it (in Slack's markup), its description no longer one to
/// validate; the others left as they are.
pub fn decided_blocks(blocks: &[Value], outcomes: &[(Uuid, String)]) -> Vec<Value> {
    let mut decided = blocks.to_vec();
    for (i, block) in blocks.iter().enumerate() {
        let named = block.get("block_id").and_then(Value::as_str);
        let Some((id, text)) = outcomes
            .iter()
            .find(|(id, _)| named == Some(format!("{BLOCK_PREFIX}{id}").as_str()))
        else {
            continue;
        };
        decided[i] = json!({
            "type": "context",
            "block_id": format!("{BLOCK_PREFIX}{id}"),
            "elements": [{ "type": "mrkdwn", "text": text }],
        });
        // Its description, two blocks above, before its exact parameters.
        if let Some(settled) = i.checked_sub(2).and_then(|d| settled(&blocks[d])) {
            decided[i - 2] = settled;
        }
    }
    decided
}

/// An action's description: its heading and `summary`, its risk, and what
/// its exact parameters below are, as `state` has them said.
fn description(state: ActionState, summary: &str, risk: &str) -> String {
    format!(
        "*{}* {summary}\n*Risque :* {risk}. {}",
        state.heading(""),
        state.parameters_heading()
    )
}

/// `block`, the description of an action to validate, as one settled;
/// `None` for any other block.
fn settled(block: &Value) -> Option<Value> {
    let open = ActionState::Open;
    let text = block.get("text")?.get("text")?.as_str()?;
    let text = text.strip_prefix(&format!("*{}* ", open.heading("")))?;
    let text = text.strip_suffix(open.parameters_heading())?;
    let (summary, risk) = text.rsplit_once("\n*Risque :* ")?;
    let risk = risk.strip_suffix(". ")?;
    Some(section(&description(ActionState::Closed, summary, risk)))
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
        // Settled, no longer one to validate; the other still is.
        let text =
            |blocks: &[Value], i: usize| blocks[i]["text"]["text"].as_str().unwrap().to_owned();
        assert_eq!(
            text(&decided, 1),
            "*Action :* Appliquer\n*Risque :* Modification. Paramètres exacts :"
        );
        assert!(text(&decided, 4).starts_with("*Action à valider :* Autre"));
        assert!(text(&decided, 4).ends_with("Paramètres exacts qui seront exécutés :"));
        let both = decided_blocks(
            &decided,
            &[(id(), "Validée".into()), (other, "Refusée".into())],
        );
        assert_eq!(both[6]["elements"][0]["text"], "Refusée");
        assert_eq!(text(&both, 1), text(&decided, 1), "settled once");
        assert!(text(&both, 4).starts_with("*Action :* Autre"));
        assert_eq!(buttons_in(&both), [id(), other], "named alike once decided");
    }
}
