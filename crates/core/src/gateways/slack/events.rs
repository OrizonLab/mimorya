//! What Slack sends to the company's application (ADR-125), once its
//! signature is verified: the check of the address Slack is given
//! (`url_verification`), or an event of its workspace. Of the events, only
//! a person's own message to the application, in their direct conversation
//! with it, reaches the agent: never an edit, a deletion or a file shared
//! (Slack names those by a subtype), never a bot's message (the
//! application's own replies among them), never a message elsewhere.

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::sync::LazyLock;

/// A workspace (`T…`), or an Enterprise Grid organisation (`E…`).
static WORKSPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[TE][A-Z0-9]{2,30}$").expect("valid regex"));
/// A person (`U…`, or `W…` across an Enterprise Grid).
static PERSON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[UW][A-Z0-9]{2,30}$").expect("valid regex"));
/// A direct conversation.
static DIRECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^D[A-Z0-9]{2,30}$").expect("valid regex"));
/// A message's timestamp, which names it in its conversation.
static TIMESTAMP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{1,12}\.[0-9]{1,9}$").expect("valid regex"));
/// An event, as Slack names it to tell its retries apart.
static EVENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Ev[A-Za-z0-9]{2,60}$").expect("valid regex"));

/// The most text taken of a message (Slack allows 40 000 characters; a
/// person's turn in the application is bounded alike).
pub const MAX_MESSAGE_CHARS: usize = 16_000;

/// What Slack sends, as far as the application reads it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Envelope {
    /// The address given to Slack is checked: the challenge is sent back.
    UrlVerification { challenge: String },
    /// An event of a workspace.
    EventCallback {
        team_id: String,
        event_id: String,
        event: Value,
    },
    /// Anything else (a rate-limit notice, a type to come): taken, ignored.
    #[serde(other)]
    Other,
}

/// A person's own message to the application, in their direct conversation
/// with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectMessage {
    /// Who wrote it (`U…`).
    pub person: String,
    /// The direct conversation (`D…`).
    pub channel: String,
    /// The message's own timestamp.
    pub ts: String,
    /// The thread it answers in, if any.
    pub thread_ts: Option<String>,
    /// Its text, as Slack writes it, bounded.
    pub text: String,
}

/// Whether `workspace` names a workspace as Slack writes it.
pub fn is_workspace(workspace: &str) -> bool {
    WORKSPACE.is_match(workspace)
}

/// Whether `person` names a person as Slack writes it.
pub fn is_person(person: &str) -> bool {
    PERSON.is_match(person)
}

/// Whether `event_id` names an event as Slack writes it.
pub fn is_event(event_id: &str) -> bool {
    EVENT.is_match(event_id)
}

/// The person's own message `event` carries, if it is one, written to the
/// application whose bot user is `bot_user`.
pub fn direct_message(event: &Value, bot_user: &str) -> Option<DirectMessage> {
    let field = |name: &str| event.get(name).and_then(Value::as_str);
    if field("type") != Some("message") || field("channel_type") != Some("im") {
        return None;
    }
    // An edit, a deletion, a file shared, a bot's message: never a person's
    // own words to the application.
    if event.get("subtype").is_some() || event.get("bot_id").is_some() {
        return None;
    }
    let person = field("user").filter(|p| PERSON.is_match(p) && *p != bot_user)?;
    let channel = field("channel").filter(|c| DIRECT.is_match(c))?;
    let ts = field("ts").filter(|t| TIMESTAMP.is_match(t))?;
    let thread_ts = match event.get("thread_ts") {
        None | Some(Value::Null) => None,
        Some(Value::String(t)) if TIMESTAMP.is_match(t) => Some(t.clone()),
        Some(_) => return None,
    };
    let text = field("text")?.trim();
    if text.is_empty() {
        return None;
    }
    Some(DirectMessage {
        person: person.to_owned(),
        channel: channel.to_owned(),
        ts: ts.to_owned(),
        thread_ts,
        text: text.chars().take(MAX_MESSAGE_CHARS).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BOT: &str = "U0BOT";

    fn message() -> Value {
        json!({
            "type": "message",
            "channel": "D024BE91L",
            "user": "U2147483697",
            "text": "Quelles interventions demain ?",
            "ts": "1355517523.000005",
            "channel_type": "im",
            "event_ts": "1355517523.000005"
        })
    }

    #[test]
    fn the_envelopes_slack_sends_are_read() {
        let check: Envelope = serde_json::from_value(json!({
            "token": "Jhj5dZrVaK7ZwHHjRyZWjbDl",
            "challenge": "3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P",
            "type": "url_verification"
        }))
        .unwrap();
        assert_eq!(
            check,
            Envelope::UrlVerification {
                challenge: "3eZbrw1aBm2rZgRNFdxV2595E9CY3gmdALWMmHkvFXO7tYXAYM8P".into()
            }
        );
        let callback: Envelope = serde_json::from_value(json!({
            "token": "XXYYZZ",
            "team_id": "T123ABC456",
            "api_app_id": "A123ABC456",
            "event": message(),
            "type": "event_callback",
            "event_id": "Ev123ABC456",
            "event_time": 1355517523
        }))
        .unwrap();
        let Envelope::EventCallback {
            team_id, event_id, ..
        } = callback
        else {
            panic!("an event");
        };
        assert_eq!(
            (team_id.as_str(), event_id.as_str()),
            ("T123ABC456", "Ev123ABC456")
        );
        let other: Envelope =
            serde_json::from_value(json!({ "type": "app_rate_limited", "minute_rate_limited": 1 }))
                .unwrap();
        assert_eq!(other, Envelope::Other);
        assert!(serde_json::from_value::<Envelope>(json!({ "challenge": "x" })).is_err());
    }

    #[test]
    fn a_person_s_own_direct_message_is_taken() {
        let taken = direct_message(&message(), BOT).unwrap();
        assert_eq!(taken.person, "U2147483697");
        assert_eq!(taken.channel, "D024BE91L");
        assert_eq!(taken.ts, "1355517523.000005");
        assert_eq!(taken.thread_ts, None);
        assert_eq!(taken.text, "Quelles interventions demain ?");

        let mut in_thread = message();
        in_thread["thread_ts"] = json!("1355517000.000001");
        let taken = direct_message(&in_thread, BOT).unwrap();
        assert_eq!(taken.thread_ts.as_deref(), Some("1355517000.000001"));

        let mut long = message();
        long["text"] = json!("é".repeat(MAX_MESSAGE_CHARS + 10));
        let taken = direct_message(&long, BOT).unwrap();
        assert_eq!(taken.text.chars().count(), MAX_MESSAGE_CHARS);
    }

    #[test]
    fn nothing_else_reaches_the_agent() {
        let changed = |change: &dyn Fn(&mut Value)| {
            let mut event = message();
            change(&mut event);
            direct_message(&event, BOT)
        };
        // Elsewhere than a direct conversation.
        assert_eq!(changed(&|e| e["channel_type"] = json!("channel")), None);
        assert_eq!(changed(&|e| e["channel"] = json!("C024BE91L")), None);
        assert_eq!(changed(&|e| e["type"] = json!("app_mention")), None);
        // An edit, a deletion, a file shared, a bot's message.
        for subtype in [
            "message_changed",
            "message_deleted",
            "file_share",
            "bot_message",
        ] {
            assert_eq!(
                changed(&|e| e["subtype"] = json!(subtype)),
                None,
                "{subtype}"
            );
        }
        assert_eq!(changed(&|e| e["bot_id"] = json!("B123")), None);
        // The application's own message.
        assert_eq!(changed(&|e| e["user"] = json!(BOT)), None);
        // Nothing said, or not as Slack writes it.
        assert_eq!(changed(&|e| e["text"] = json!("   ")), None);
        assert_eq!(changed(&|e| e["text"] = json!(12)), None);
        assert_eq!(changed(&|e| e["user"] = json!("u2147483697")), None);
        assert_eq!(changed(&|e| e["ts"] = json!("1355517523")), None);
        assert_eq!(changed(&|e| e["thread_ts"] = json!("x")), None);
        assert_eq!(changed(&|e| e["thread_ts"] = json!(1)), None);
        assert_eq!(
            changed(&|e| {
                e.as_object_mut().unwrap().remove("user");
            }),
            None
        );
    }

    #[test]
    fn identifiers_are_read_as_slack_writes_them() {
        assert!(is_workspace("T123ABC456") && is_workspace("E0ORG"));
        assert!(!is_workspace("t123") && !is_workspace("T") && !is_workspace("T12 3"));
        assert!(is_person("U2147483697") && is_person("W0GRID"));
        assert!(!is_person("B123") && !is_person("U2147483697\n"));
        assert!(is_event("Ev123ABC456") && !is_event("ev123") && !is_event("Ev"));
    }
}
