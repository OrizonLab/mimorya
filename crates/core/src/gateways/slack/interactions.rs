//! The validations given in Slack (ADR-129): a run started there that
//! pauses for its author's own validation shows, in its thread, each action
//! awaiting it with a « Valider » and a « Refuser » button; Slack sends the
//! application which button a person pressed (`block_actions`), signed as
//! its events. Only those buttons are read: their approval, the person who
//! pressed, where the message is. The message, once decided, shows what
//! became of the action in place of its buttons.

use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;
use uuid::Uuid;

use super::events::{is_person, is_workspace};
use super::mrkdwn::{bounded, escaped};

/// The buttons, as the application names them.
pub const APPROVE_ACTION: &str = "approval_approve";
pub const REJECT_ACTION: &str = "approval_reject";
/// How an action's buttons are named in their message, before its approval.
const BLOCK_PREFIX: &str = "approval:";
/// The most of an action's description shown in Slack.
pub const MAX_ACTION_CHARS: usize = 600;
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
/// action awaiting its author's validation, its description (plain text,
/// escaped and bounded) with its two buttons.
pub fn approval_blocks(intro: &str, actions: &[(Uuid, &str)]) -> Vec<Value> {
    let mut blocks = vec![section(intro)];
    for (id, description) in actions {
        let (shown, _) = bounded(description.trim(), MAX_ACTION_CHARS);
        blocks.push(section(&format!(
            "*Action à valider :* {}",
            escaped(&shown)
        )));
        let value = id.to_string();
        blocks.push(json!({
            "type": "actions",
            "block_id": format!("{BLOCK_PREFIX}{id}"),
            "elements": [
                button("Valider", APPROVE_ACTION, &value, "primary"),
                button("Refuser", REJECT_ACTION, &value, "danger"),
            ],
        }));
    }
    blocks
}

/// `blocks` with the buttons of `approval_id` replaced by `outcome` (in
/// Slack's markup); `None` if they are not there.
pub fn decided_blocks(blocks: &[Value], approval_id: Uuid, outcome: &str) -> Option<Vec<Value>> {
    let named = format!("{BLOCK_PREFIX}{approval_id}");
    let at = blocks
        .iter()
        .position(|b| b.get("block_id").and_then(Value::as_str) == Some(named.as_str()))?;
    let mut decided = blocks.to_vec();
    decided[at] = json!({
        "type": "context",
        "block_id": named,
        "elements": [{ "type": "mrkdwn", "text": outcome }],
    });
    Some(decided)
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

    fn pressed(action_id: &str, value: &str) -> Value {
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
            "message": { "blocks": approval_blocks("Pause", &[(Uuid::parse_str(APPROVAL).unwrap(), "Envoyer")]) },
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
        assert_eq!(choice.approval_id.to_string(), APPROVAL);
        assert!(choice.approved);
        assert_eq!(choice.blocks.len(), 3);
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
    fn an_action_s_buttons_carry_its_approval_and_its_description_escaped() {
        let id = Uuid::parse_str(APPROVAL).unwrap();
        let blocks = approval_blocks("Une action attend", &[(id, "Écrire à <!channel> & co")]);
        assert_eq!(
            blocks[1]["text"]["text"],
            "*Action à valider :* Écrire à &lt;!channel&gt; &amp; co"
        );
        assert_eq!(blocks[2]["block_id"], format!("approval:{APPROVAL}"));
        assert_eq!(blocks[2]["elements"][0]["action_id"], APPROVE_ACTION);
        assert_eq!(blocks[2]["elements"][0]["value"], APPROVAL);
        assert_eq!(blocks[2]["elements"][1]["action_id"], REJECT_ACTION);
        let long = "x".repeat(MAX_ACTION_CHARS * 2);
        let bounded = approval_blocks("Pause", &[(id, &long)]);
        let shown = bounded[1]["text"]["text"].as_str().unwrap();
        assert!(shown.chars().count() < MAX_ACTION_CHARS + 40, "{shown}");
    }

    #[test]
    fn a_decided_action_shows_its_outcome_in_place_of_its_buttons() {
        let id = Uuid::parse_str(APPROVAL).unwrap();
        let blocks = approval_blocks("Pause", &[(id, "Envoyer")]);
        let decided = decided_blocks(&blocks, id, "Validée").unwrap();
        assert_eq!(decided.len(), 3);
        assert_eq!(decided[2]["type"], "context");
        assert_eq!(decided[2]["elements"][0]["text"], "Validée");
        assert_eq!(decided_blocks(&blocks, Uuid::new_v4(), "Validée"), None);
    }
}
