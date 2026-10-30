use serde_json::{Value, json};

use super::*;

const APP: &str = "6f1c2b7a-0d3e-4c5b-9a8f-7e6d5c4b3a21";
const DIRECTORY: &str = "0b2c3d4e-5f60-4718-8293-a4b5c6d7e8f9";
const PERSON: &str = "d6b9e1f2-3a4b-4c5d-8e9f-0a1b2c3d4e5f";

fn activity() -> Value {
    json!({
        "type": "message",
        "id": "1700000000123",
        "serviceUrl": "https://smba.trafficmanager.net/emea/",
        "channelId": "msteams",
        "from": { "id": "29:1abcDEF", "name": "Marc Lefèvre", "aadObjectId": PERSON },
        "conversation": {
            "conversationType": "personal",
            "tenantId": DIRECTORY,
            "id": "a:1xyz"
        },
        "recipient": { "id": format!("28:{APP}"), "name": "Assistant" },
        "text": " Bonjour ",
        "channelData": { "tenant": { "id": DIRECTORY } }
    })
}

fn message(activity: &Value) -> Option<PersonalMessage> {
    read_activity(activity.to_string().as_bytes())?.personal_message(APP, DIRECTORY)
}

#[test]
fn a_person_s_own_message_to_the_bot_is_read() {
    let read = message(&activity()).expect("a personal message");
    assert_eq!(read.activity_id, "1700000000123");
    assert_eq!(read.service_url, "https://smba.trafficmanager.net/emea/");
    assert_eq!(read.conversation_id, "a:1xyz");
    assert_eq!(read.text, "Bonjour");
    assert_eq!(
        read.person,
        TeamsUser {
            id: "29:1abcDEF".into(),
            aad_object_id: PERSON.into(),
            name: Some("Marc Lefèvre".into()),
        }
    );
    // The directory written otherwise, or only in the conversation.
    let mut other = activity();
    other["from"]["aadObjectId"] = json!(PERSON.to_uppercase());
    other["channelData"] = json!({});
    other["conversation"]["tenantId"] = json!(DIRECTORY.to_uppercase());
    assert_eq!(
        message(&other).map(|m| m.person.aad_object_id),
        Some(PERSON.into())
    );
}

#[test]
fn anything_but_a_person_s_own_message_to_the_bot_in_the_directory_is_ignored() {
    let ignored = |change: &dyn Fn(&mut Value)| {
        let mut activity = activity();
        change(&mut activity);
        message(&activity)
    };
    assert_eq!(ignored(&|a| a["type"] = json!("conversationUpdate")), None);
    assert_eq!(ignored(&|a| a["channelId"] = json!("slack")), None);
    assert_eq!(
        ignored(&|a| a["conversation"]["conversationType"] = json!("groupChat")),
        None
    );
    assert_eq!(
        ignored(&|a| a["conversation"]["conversationType"] = json!("channel")),
        None
    );
    // For another bot, or from another directory.
    assert_eq!(ignored(&|a| a["recipient"]["id"] = json!("28:autre")), None);
    assert_eq!(
        ignored(&|a| {
            a["channelData"]["tenant"]["id"] = json!("11111111-2222-4333-8444-555555555555");
        }),
        None
    );
    assert_eq!(
        ignored(&|a| {
            a["channelData"] = json!({});
            a["conversation"]["tenantId"] = Value::Null;
        }),
        None
    );
    // Without a person to name, or naming them otherwise than Teams.
    assert_eq!(ignored(&|a| a["from"]["aadObjectId"] = Value::Null), None);
    assert_eq!(
        ignored(&|a| a["from"]["aadObjectId"] = json!("pas-un-guid")),
        None
    );
    assert_eq!(ignored(&|a| a["from"]["id"] = json!("28:bot")), None);
    // Identifiers out of bounds, a message too long.
    assert_eq!(ignored(&|a| a["id"] = json!("x".repeat(65))), None);
    assert_eq!(ignored(&|a| a["id"] = json!("a b")), None);
    assert_eq!(ignored(&|a| a["conversation"]["id"] = json!("")), None);
    assert_eq!(
        ignored(&|a| a["text"] = json!("é".repeat(MAX_MESSAGE_CHARS + 1))),
        None
    );
}

#[test]
fn an_activity_names_its_service_first() {
    let mut without = activity();
    without["serviceUrl"] = Value::Null;
    assert!(read_activity(without.to_string().as_bytes()).is_none());
    assert!(read_activity(b"pas du json").is_none());
    let read = read_activity(activity().to_string().as_bytes()).unwrap();
    assert_eq!(read.service_url, "https://smba.trafficmanager.net/emea/");
}

#[test]
fn an_entra_identifier_is_read_as_microsoft_writes_it() {
    assert_eq!(guid(&PERSON.to_uppercase()).as_deref(), Some(PERSON));
    assert_eq!(guid(" pas "), None);
    // Braced or without hyphens: not as Microsoft writes them.
    assert_eq!(guid(&format!("{{{PERSON}}}")), None);
    assert_eq!(guid(&PERSON.replace('-', "")), None);
    assert!(same_guid(DIRECTORY, &DIRECTORY.to_uppercase()));
    assert!(!same_guid(DIRECTORY, PERSON));
    assert!(!same_guid("x", "x"));
}

#[test]
fn a_message_is_taken_once_in_its_conversation_only() {
    let first = message(&activity()).unwrap();
    assert_eq!(first.event_key().len(), 64);
    assert_eq!(first.event_key(), message(&activity()).unwrap().event_key());
    // The same number in another conversation is another message.
    let mut elsewhere = activity();
    elsewhere["conversation"]["id"] = json!("a:2abc");
    assert_ne!(first.event_key(), message(&elsewhere).unwrap().event_key());
}

#[test]
fn a_button_pressed_on_a_card_is_read_with_the_card_it_answers() {
    let approval = "0d6f8a1b-2c3d-4e5f-8a9b-0c1d2e3f4a5b";
    let mut pressed = activity();
    pressed["text"] = Value::Null;
    pressed["value"] = json!({ "mimorya": "v", "approval": approval });
    pressed["replyToId"] = json!("1:carte-42");
    let read = message(&pressed).expect("a personal message");
    assert_eq!(read.text, "");
    assert_eq!(read.reply_to_id.as_deref(), Some("1:carte-42"));
    assert_eq!(
        read.choice,
        Some(CardChoice {
            approval_id: approval.parse().unwrap(),
            approved: true,
        })
    );
    // Words written are no choice; a value of another kind neither.
    let written = message(&activity()).unwrap();
    assert_eq!((written.choice, written.reply_to_id), (None, None));
    pressed["value"] = json!({ "action": "submit" });
    assert_eq!(message(&pressed).unwrap().choice, None);
}
