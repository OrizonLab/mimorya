//! The Google Workspace tools that write (ADR-123): offered when the
//! company lets them, always validated, refused before validation when what
//! they would send hides something, sent once to Gmail or Calendar exactly
//! as validated with the person's token, and saying when Google may have
//! acted all the same.

mod support;

use std::sync::Arc;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolExecutor};
use mimorya_connectors::google_workspace::{GOOGLE_WORKSPACE_TOOLS, GOOGLE_WORKSPACE_WRITE_TOOLS};
use mimorya_connectors::testing::{FakeGoogle, GOOGLE_DETAIL, SentMail};
use mimorya_core::account_links::GOOGLE_WORKSPACE;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::LinkedAccounts;
use serde_json::{Value, json};
use support::google_workspace::{google_tools, google_writing_tools};
use support::linked_accounts::{error_of, mailbox_executor, validated_call};
use uuid::Uuid;

const TOKEN: &str = "acces-lea-1";

async fn lea(google: &FakeGoogle) -> (ToolExecutor, Arc<InMemoryPorts>) {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account(GOOGLE_WORKSPACE, TOKEN);
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let tools = google_writing_tools(google).await;
    (mailbox_executor(tools, user::LEA, &ports, accounts), ports)
}

fn email() -> Value {
    json!({
        "to": ["paul@exemple.fr", "marc@exemple.fr"],
        "cc": ["claire@exemple.fr"],
        "subject": "Planning de novembre — équipe",
        "body": "Bonjour,\n\nLe planning est prêt.\n\tLéa",
    })
}

fn evaluated(executor: &ToolExecutor, name: &str, input: Value) -> ToolCallEvaluation {
    executor.evaluate(&LlmToolCall {
        id: "c".into(),
        name: name.into(),
        input,
    })
}

#[tokio::test]
async fn writing_tools_are_offered_when_the_company_lets_them_and_always_validated() {
    let google = FakeGoogle::start().await;
    // Reading only, unless the company lets them write.
    let reading: Vec<String> = google_tools(&google)
        .await
        .iter()
        .map(|t| t.info().name.clone())
        .collect();
    assert_eq!(reading, GOOGLE_WORKSPACE_TOOLS[..3]);
    let tools = google_writing_tools(&google).await;
    let names: Vec<String> = tools.iter().map(|t| t.info().name.clone()).collect();
    assert_eq!(names, GOOGLE_WORKSPACE_TOOLS);
    for tool in tools
        .iter()
        .filter(|t| GOOGLE_WORKSPACE_WRITE_TOOLS.contains(&t.info().name.as_str()))
    {
        let info = tool.info();
        assert_eq!(info.risk, ToolRisk::External, "{}", info.name);
        assert_eq!(info.required_permissions, ["connector:use"]);
        assert!(info.requires_approval, "{}", info.name);
    }
    let send = tools
        .iter()
        .find(|t| t.info().name == "google_gmail_send")
        .unwrap();
    let described = send.describe_action(&email()).unwrap();
    assert!(
        described.starts_with(
            "Envoyer depuis la boîte Google Workspace reliée un e-mail à paul@exemple.fr, \
             marc@exemple.fr (copie : claire@exemple.fr) — objet : « Planning de novembre"
        ),
        "{described}"
    );
    assert!(described.contains("Le planning est prêt."), "{described}");
}

#[tokio::test]
async fn an_email_is_sent_once_as_validated_from_the_persons_gmail() {
    let google = FakeGoogle::start().await;
    let (executor, _) = lea(&google).await;
    let result = validated_call(&executor, "google_gmail_send", email()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["envoye"], true);
    assert_eq!(result.content["id"], "lea-envoye");
    assert_eq!(
        result.content["lien"],
        "https://mail.google.com/mail/#all/lea-envoye"
    );
    assert_eq!(result.citations[0].kind, "email");
    assert_eq!(
        result.citations[0].label.as_deref(),
        Some("Planning de novembre — équipe")
    );
    let sent = google.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].token, TOKEN);
    assert_eq!(sent[0].path, "/users/me/messages/send");
    let body = sent[0].body.as_ref().unwrap();
    assert_eq!(body.as_object().unwrap().len(), 1, "{body}");
    let mail = SentMail::read(body["raw"].as_str().unwrap()).unwrap();
    let headers: Vec<&str> = mail.headers.iter().map(|(n, _)| n.as_str()).collect();
    // Gmail adds the sender: the person whose token sends it.
    assert_eq!(
        headers,
        [
            "To",
            "Cc",
            "Subject",
            "MIME-Version",
            "Content-Type",
            "Content-Transfer-Encoding"
        ]
    );
    assert_eq!(mail.header("to"), Some("paul@exemple.fr, marc@exemple.fr"));
    assert_eq!(mail.header("cc"), Some("claire@exemple.fr"));
    assert_eq!(
        mail.header("subject"),
        Some("Planning de novembre — équipe")
    );
    assert_eq!(
        mail.header("content-type"),
        Some("text/plain; charset=\"UTF-8\"")
    );
    assert_eq!(mail.text, "Bonjour,\r\n\r\nLe planning est prêt.\r\n\tLéa");
}

#[tokio::test]
async fn what_would_hide_something_is_refused_before_validation() {
    let google = FakeGoogle::start().await;
    let (executor, _) = lea(&google).await;
    let event =
        json!({ "subject": "Point", "start": "2026-10-28T10:00:00", "durationMinutes": 30 });
    let send = "google_gmail_send";
    let create = "google_calendar_event_create";
    let refused = [
        (
            send,
            email(),
            "subject",
            json!("Objet\r\nBcc: tous@exemple.fr"),
        ),
        (
            send,
            email(),
            "body",
            json!("Bonjour\u{202E}, voir le lien"),
        ),
        (send, email(), "to", json!(["paul@ex\u{435}mple.fr"])),
        (
            send,
            email(),
            "cc",
            json!(["claire@exemple.fr\nBcc: x@y.fr"]),
        ),
        (
            create,
            event.clone(),
            "location",
            json!("Salle\u{200B} Atlas"),
        ),
        (
            create,
            event.clone(),
            "attendees",
            json!(["paul@exemple.fr", "PAUL@exemple.fr"]),
        ),
        (create, event.clone(), "start", json!("2026-03-29T02:30:00")),
        (create, event.clone(), "durationMinutes", json!(1441)),
    ];
    for (tool, mut input, field, value) in refused {
        input[field] = value;
        let evaluation = evaluated(&executor, tool, input.clone());
        assert!(
            matches!(evaluation, ToolCallEvaluation::Deny { .. }),
            "{tool} {input}"
        );
    }
    // As given, each asks its validation: the field alone was refused.
    for (tool, input) in [(send, email()), (create, event)] {
        let evaluation = evaluated(&executor, tool, input);
        assert!(
            matches!(evaluation, ToolCallEvaluation::RequireApproval { .. }),
            "{tool}"
        );
    }
    assert!(google.requests().is_empty());
}

#[tokio::test]
async fn an_event_is_created_in_the_platforms_zone_with_its_attendees_invited() {
    let google = FakeGoogle::start().await;
    let (executor, _) = lea(&google).await;
    let input = json!({
        "subject": "Visite du chantier",
        "start": "2026-11-03T09:30:00",
        "durationMinutes": 90,
        "location": "Chantier Atlas",
        "body": "Casques fournis.",
        "attendees": ["paul@exemple.fr"],
    });
    let result = validated_call(&executor, "google_calendar_event_create", input).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["cree"], true);
    assert_eq!(result.content["id"], "lea-nouveau");
    // Given back in the calendar's zone.
    assert_eq!(result.content["debut"], "2026-11-03T09:30:00+01:00");
    assert_eq!(result.content["fin"], "2026-11-03T11:00:00+01:00");
    assert_eq!(
        result.content["lien"],
        "https://calendar.example/lea/nouveau"
    );
    assert_eq!(result.citations[0].kind, "event");
    assert_eq!(result.citations[0].id, "lea-nouveau");
    let created = &google.requests()[0];
    assert_eq!(created.path, "/calendars/primary/events");
    assert_eq!(created.param("sendUpdates"), Some("all"));
    assert_eq!(
        created.body,
        Some(json!({
            "summary": "Visite du chantier",
            "start": { "dateTime": "2026-11-03T08:30:00Z", "timeZone": "Europe/Paris" },
            "end": { "dateTime": "2026-11-03T10:00:00Z", "timeZone": "Europe/Paris" },
            "location": "Chantier Atlas",
            "description": "Casques fournis.",
            "attendees": [{ "email": "paul@exemple.fr" }],
        }))
    );
}

#[tokio::test]
async fn a_write_google_may_have_done_is_told_to_be_checked_first() {
    let google = FakeGoogle::start().await;
    let (executor, _) = lea(&google).await;
    google.refuse_writes(503);
    let failing = error_of(&validated_call(&executor, "google_gmail_send", email()).await);
    assert!(failing.contains("a peut-être eu lieu"), "{failing}");
    assert!(
        failing.contains("vérifiez dans Google Workspace"),
        "{failing}"
    );
    google.refuse_writes(400);
    let refused = error_of(&validated_call(&executor, "google_gmail_send", email()).await);
    assert!(!refused.contains("peut-être"), "{refused}");
    google.refuse_writes(403);
    let forbidden = error_of(&validated_call(&executor, "google_gmail_send", email()).await);
    assert!(forbidden.contains("permission"), "{forbidden}");
    for told in [failing, refused, forbidden] {
        assert!(!told.contains(GOOGLE_DETAIL), "{told}");
    }
    // Each validated once, sent once: never repeated.
    assert_eq!(google.requests().len(), 3);
}

#[tokio::test]
async fn a_token_google_refuses_on_a_write_is_reported() {
    let google = FakeGoogle::start().await;
    let (executor, ports) = lea(&google).await;
    google.refuse(TOKEN);
    let event =
        json!({ "subject": "Point", "start": "2026-11-03T09:30:00", "durationMinutes": 30 });
    let error = error_of(&validated_call(&executor, "google_calendar_event_create", event).await);
    assert!(error.contains("« Mon compte »"), "{error}");
    assert_eq!(ports.rejected_accounts(), [GOOGLE_WORKSPACE]);
}
