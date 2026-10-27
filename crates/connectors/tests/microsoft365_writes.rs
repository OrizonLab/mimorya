//! The Microsoft 365 tools that write (ADR-119): offered when the company
//! lets them, always validated, refused before validation when what they
//! would send hides something, sent once to Graph exactly as validated
//! with the person's token, and saying when Graph may have acted all the
//! same.

mod support;

use std::sync::Arc;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::{ToolCallEvaluation, ToolExecutor};
use mimorya_connectors::microsoft365::{MICROSOFT365_TOOLS, MICROSOFT365_WRITE_TOOLS};
use mimorya_connectors::testing::{FakeGraph, GRAPH_DETAIL};
use mimorya_core::account_links::{MAIL_SEND, MICROSOFT365};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use serde_json::{Value, json};
use support::microsoft365::{
    Unreachable, error_of, mailbox_executor, microsoft365_writing_tools, validated_call,
};
use uuid::Uuid;

const TOKEN: &str = "acces-lea-1";

async fn lea(graph: &FakeGraph) -> (ToolExecutor, Arc<InMemoryPorts>) {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account(MICROSOFT365, TOKEN);
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let tools = microsoft365_writing_tools(&graph.url).await;
    (mailbox_executor(tools, user::LEA, &ports, accounts), ports)
}

fn email() -> Value {
    json!({
        "to": ["paul@exemple.fr", "marc@exemple.fr"],
        "cc": ["claire@exemple.fr"],
        "subject": "Planning de novembre",
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
    let graph = FakeGraph::start().await;
    let tools = microsoft365_writing_tools(&graph.url).await;
    let names: Vec<String> = tools.iter().map(|t| t.info().name.clone()).collect();
    assert_eq!(names, MICROSOFT365_TOOLS);
    for tool in tools
        .iter()
        .filter(|t| MICROSOFT365_WRITE_TOOLS.contains(&t.info().name.as_str()))
    {
        let info = tool.info();
        assert_eq!(info.risk, ToolRisk::External, "{}", info.name);
        assert_eq!(info.required_permissions, ["connector:use"]);
        assert!(info.requires_approval, "{}", info.name);
    }
    let send = tools
        .iter()
        .find(|t| t.info().name == "m365_mail_send")
        .unwrap();
    let described = send.describe_action(&email()).unwrap();
    assert!(
        described.contains("« Planning de novembre »"),
        "{described}"
    );
    assert!(
        described.contains("paul@exemple.fr, marc@exemple.fr"),
        "{described}"
    );
    assert!(
        described.contains("(copie : claire@exemple.fr)"),
        "{described}"
    );
    assert!(described.contains("Le planning est prêt."), "{described}");
}

#[tokio::test]
async fn an_email_is_sent_once_as_validated_from_the_persons_mailbox() {
    let graph = FakeGraph::start().await;
    let (executor, _) = lea(&graph).await;
    let result = validated_call(&executor, "m365_mail_send", email()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["envoye"], true);
    let sent = graph.requests();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].token, TOKEN);
    assert_eq!(sent[0].path, "/me/sendMail");
    assert_eq!(
        sent[0].body,
        Some(json!({
            "message": {
                "subject": "Planning de novembre",
                "body": { "contentType": "Text", "content": "Bonjour,\n\nLe planning est prêt.\n\tLéa" },
                "toRecipients": [
                    { "emailAddress": { "address": "paul@exemple.fr" } },
                    { "emailAddress": { "address": "marc@exemple.fr" } },
                ],
                "ccRecipients": [{ "emailAddress": { "address": "claire@exemple.fr" } }],
            },
            "saveToSentItems": true,
        }))
    );
}

#[tokio::test]
async fn what_would_hide_something_is_refused_before_validation() {
    let graph = FakeGraph::start().await;
    let (executor, _) = lea(&graph).await;
    let mut hidden = email();
    hidden["body"] = json!("Bonjour\u{202E}, voir pièce jointe");
    let mut invalid = email();
    invalid["to"] = json!(["paul"]);
    let mut many = email();
    many["to"] = json!(
        (0..11)
            .map(|n| format!("p{n}@exemple.fr"))
            .collect::<Vec<_>>()
    );
    let mut two_lines = email();
    two_lines["subject"] = json!("Objet\nBcc: tous@exemple.fr");
    for input in [hidden, invalid, many, two_lines] {
        let evaluation = evaluated(&executor, "m365_mail_send", input.clone());
        assert!(
            matches!(evaluation, ToolCallEvaluation::Deny { .. }),
            "{input}"
        );
    }
    let event = json!({
        "subject": "Point",
        "start": "2026-10-28T10:00:00",
        "durationMinutes": 30,
        "attendees": ["paul@exemple.fr", "Paul@exemple.fr"],
    });
    assert!(matches!(
        evaluated(&executor, "m365_calendar_event_create", event),
        ToolCallEvaluation::Deny { .. }
    ));
    assert!(graph.requests().is_empty());
}

#[tokio::test]
async fn an_event_is_created_in_the_platforms_zone_with_its_attendees() {
    let graph = FakeGraph::start().await;
    let (executor, _) = lea(&graph).await;
    let input = json!({
        "subject": "Visite du chantier",
        "start": "2026-11-03T09:30:00",
        "durationMinutes": 90,
        "location": "Chantier Atlas",
        "body": "Casques fournis.",
        "attendees": ["paul@exemple.fr"],
    });
    let result = validated_call(&executor, "m365_calendar_event_create", input).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["cree"], true);
    assert_eq!(result.content["id"], "lea-nouveau");
    assert_eq!(result.content["debut"], "2026-11-03T09:30:00");
    assert_eq!(result.citations[0].kind, "event");
    assert_eq!(result.citations[0].id, "lea-nouveau");
    let created = &graph.requests()[0];
    assert_eq!(created.path, "/me/events");
    assert_eq!(
        created.prefer.as_deref(),
        Some("outlook.timezone=\"Europe/Paris\"")
    );
    assert_eq!(
        created.body,
        Some(json!({
            "subject": "Visite du chantier",
            "start": { "dateTime": "2026-11-03T09:30:00", "timeZone": "Europe/Paris" },
            "end": { "dateTime": "2026-11-03T11:00:00", "timeZone": "Europe/Paris" },
            "location": { "displayName": "Chantier Atlas" },
            "body": { "contentType": "Text", "content": "Casques fournis." },
            "attendees": [
                { "emailAddress": { "address": "paul@exemple.fr" }, "type": "required" },
            ],
        }))
    );
}

#[tokio::test]
async fn a_write_graph_may_have_done_is_told_to_be_checked_first() {
    let graph = FakeGraph::start().await;
    let (executor, _) = lea(&graph).await;
    graph.refuse_writes(503);
    let failing = error_of(&validated_call(&executor, "m365_mail_send", email()).await);
    assert!(failing.contains("a peut-être eu lieu"), "{failing}");
    assert!(failing.contains("vérifiez"), "{failing}");
    graph.refuse_writes(400);
    let refused = error_of(&validated_call(&executor, "m365_mail_send", email()).await);
    assert!(!refused.contains("peut-être"), "{refused}");
    graph.refuse_writes(403);
    let forbidden = error_of(&validated_call(&executor, "m365_mail_send", email()).await);
    assert!(forbidden.contains("permission"), "{forbidden}");
    for told in [failing, refused, forbidden] {
        assert!(!told.contains(GRAPH_DETAIL), "{told}");
    }
    // Each validated once, sent once: never repeated.
    assert_eq!(graph.requests().len(), 3);
}

#[tokio::test]
async fn a_token_graph_refuses_on_a_write_is_reported() {
    let graph = FakeGraph::start().await;
    let (executor, ports) = lea(&graph).await;
    graph.refuse(TOKEN);
    let error = error_of(&validated_call(&executor, "m365_mail_send", email()).await);
    assert!(error.contains("« Mon compte »"), "{error}");
    assert_eq!(ports.rejected_accounts(), [MICROSOFT365]);
}

#[tokio::test]
async fn a_permission_the_person_did_not_grant_reaches_nothing() {
    let graph = FakeGraph::start().await;
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    let not_granted = AccountAccessError::NotGranted(vec![MAIL_SEND.into()]);
    let accounts: Arc<dyn LinkedAccounts> = Arc::new(Unreachable(not_granted));
    let tools = microsoft365_writing_tools(&graph.url).await;
    let executor = mailbox_executor(tools, user::LEA, &ports, accounts);
    let error = error_of(&validated_call(&executor, "m365_mail_send", email()).await);
    assert!(error.contains("l’envoi d’e-mails en son nom"), "{error}");
    assert!(
        error.contains("relie de nouveau son compte dans « Mon compte »"),
        "{error}"
    );
    assert!(graph.requests().is_empty());
}
