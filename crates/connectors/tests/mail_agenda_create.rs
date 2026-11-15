//! The standard mail's `mail_calendar_event_create` (ADR-205): offered
//! where the company declared an agenda and lets its tools write, always
//! validated; the event put as validated into the first agenda of the
//! person the run serves that takes events (or the one their server names
//! for new events: `mail_agenda_default.rs`), as a new object only, its
//! attendees invited by the server from an address it knows the account
//! by, nothing the model wrote adding a property or an event. What is said
//! of failures is tried in `mail_agenda_create_failures.rs`.

mod support;

use std::time::Duration;

use mimorya_agent::llm::LlmToolCall;
use mimorya_agent::tools::ToolCallEvaluation;
use mimorya_connectors::testing::{
    AGENDA_PATH, CalDavBehaviour, CreatedObject, FakeCalDav, READ_ONLY_PATH, TestAuthority,
};
use mimorya_core::autonomy::ToolRisk;
use serde_json::{Value, json};
use support::agenda::{declaration, new_event, paul_writing_at};
use support::linked_accounts::validated_call;
use support::mail::{USER, accounts};
use support::sending::tools;

const CREATE: &str = "mail_calendar_event_create";

#[tokio::test]
async fn creating_is_offered_where_the_company_declared_an_agenda_and_lets_it_write() {
    let authority = TestAuthority::new();
    let agenda = FakeCalDav::start(&authority, accounts(), CalDavBehaviour::Answers, vec![]).await;
    let mut declared = declaration(&agenda);
    let reading = tools(&authority, &declared, Duration::from_secs(5)).await;
    assert!(reading.iter().all(|t| t.info().name != CREATE));
    declared["writeTools"] = json!(true);
    declared["caldav"] = Value::Null;
    let no_agenda = tools(&authority, &declared, Duration::from_secs(5)).await;
    assert!(no_agenda.iter().all(|t| t.info().name != CREATE));
    declared["caldav"] = json!(agenda.base());
    let writing = tools(&authority, &declared, Duration::from_secs(5)).await;
    let create = writing.iter().find(|t| t.info().name == CREATE).unwrap();
    let info = create.info();
    assert_eq!(info.risk, ToolRisk::External);
    assert_eq!(info.required_permissions, ["connector:use"]);
    assert!(info.requires_approval);
    let input = serde_json::from_value(new_event()).unwrap();
    let described = create.describe_action(&input).unwrap();
    assert!(
        described.starts_with(
            "Créer dans l’agenda de messagerie relié un rendez-vous le 28/11/2026 à 23:30 \
             (heure de Paris), pendant 90 min, invitation envoyée à claire@exemple.fr, \
             l.martin+chantier@exemple.fr — objet : « Point chantier ; lot B, révisé »"
        ),
        "{described}"
    );
}

/// The one object the server keeps.
fn the_one(agenda: &FakeCalDav) -> CreatedObject {
    let created = agenda.created.all();
    assert_eq!(created.len(), 1, "{created:?}");
    created[0].clone()
}

#[tokio::test]
async fn the_event_is_created_as_validated_in_the_first_agenda_that_takes_it() {
    let behaviour = CalDavBehaviour::ReadOnlyFirst;
    let (agenda, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let result = validated_call(&paul, CREATE, new_event()).await;
    assert!(!result.is_error, "{:?}", result.content);
    let said = &result.content;
    assert_eq!(said["cree"], true);
    assert_eq!(said["agenda"], "Personnel");
    assert_eq!(said["debut"], "2026-11-28T23:30:00+01:00");
    assert_eq!(said["fin"], "2026-11-29T01:00:00+01:00");
    assert_eq!(said["organisateur"], USER);
    assert_eq!(
        said["participants"],
        json!(["claire@exemple.fr", "l.martin+chantier@exemple.fr"])
    );
    assert_eq!(result.summary.as_deref(), Some("Rendez-vous créé"));
    let created = the_one(&agenda);
    let id = said["id"].as_str().unwrap();
    assert_eq!(result.citations[0].id, id);
    // A new object of its own, in the agenda that takes events, never over
    // another.
    assert_eq!(created.path, format!("{AGENDA_PATH}{id}.ics"));
    assert_eq!(created.if_none_match.as_deref(), Some("*"));
    assert_eq!(
        created.content_type.as_deref(),
        Some("text/calendar; charset=utf-8")
    );
    // Lines past 75 octets are folded (RFC 5545, 3.1): read unfolded.
    let text = &created.text.replace("\r\n ", "");
    for line in [
        format!("UID:{id}"),
        "DTSTART:20261128T223000Z".into(),
        "DTEND:20261129T000000Z".into(),
        "SUMMARY:Point chantier \\; lot B\\, révisé".into(),
        "LOCATION:Quai B".into(),
        "DESCRIPTION:Ordre du jour :\\n\tsécurité".into(),
        format!("ORGANIZER:mailto:{USER}"),
        "ATTENDEE;ROLE=REQ-PARTICIPANT;PARTSTAT=NEEDS-ACTION;RSVP=TRUE:mailto:claire@exemple.fr"
            .into(),
    ] {
        assert!(
            text.contains(&format!("\r\n{line}\r\n")),
            "{line} in {text}"
        );
    }
    // Once the agendas are found, whether the server invites, and from
    // where, asked first; the agenda it only reads never written.
    let heard = agenda.heard.lines();
    let listed = heard
        .iter()
        .position(|line| line == "PROPFIND /dav/calendars/paul/")
        .unwrap();
    assert_eq!(
        heard[listed + 1..],
        [
            format!("OPTIONS {AGENDA_PATH}"),
            "PROPFIND /dav/principals/paul/".into(),
            format!("PUT {AGENDA_PATH}{id}.ics"),
        ]
    );
    assert!(heard.iter().all(|line| !line.contains(READ_ONLY_PATH)));
}

#[tokio::test]
async fn nothing_the_model_writes_adds_a_property_or_an_event() {
    let (agenda, _, paul) =
        paul_writing_at(CalDavBehaviour::Answers, Duration::from_secs(20)).await;
    let body = "Bonjour\nEND:VEVENT\nBEGIN:VEVENT\nATTENDEE:mailto:tous@exemple.fr\n\
                ORGANIZER:mailto:direction@exemple.fr";
    let mut forged = new_event();
    forged["body"] = json!(body);
    let result = validated_call(&paul, CREATE, forged).await;
    assert!(!result.is_error, "{:?}", result.content);
    let text = the_one(&agenda).text;
    let unfolded = text.replace("\r\n ", "");
    let lines: Vec<&str> = unfolded.split("\r\n").collect();
    for one in ["BEGIN:VEVENT", "END:VEVENT"] {
        assert_eq!(
            lines.iter().filter(|line| **line == one).count(),
            1,
            "{text}"
        );
    }
    let properties: Vec<&&str> = lines
        .iter()
        .filter(|line| line.starts_with("ATTENDEE") || line.starts_with("ORGANIZER"))
        .collect();
    assert_eq!(properties.len(), 3, "{text}");
    assert!(
        properties
            .iter()
            .all(|line| !line.contains("tous@") && !line.contains("direction@"))
    );
    // A subject that would end its line is refused before any request.
    let mut ended = new_event();
    ended["subject"] = json!("Point\r\nATTENDEE:mailto:tous@exemple.fr");
    let evaluation = paul.evaluate(&LlmToolCall {
        id: "c".into(),
        name: CREATE.into(),
        input: ended,
    });
    assert!(matches!(evaluation, ToolCallEvaluation::Deny { .. }));
    assert_eq!(agenda.created.all().len(), 1);
}

#[tokio::test]
async fn an_event_without_attendees_invites_no_one_and_asks_no_scheduling() {
    // A server that would not invite anyone takes an event with no one to
    // invite.
    let behaviour = CalDavBehaviour::NoScheduling;
    let (agenda, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let mut alone = new_event();
    alone["attendees"] = json!([]);
    let result = validated_call(&paul, CREATE, alone).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["organisateur"], Value::Null);
    let text = the_one(&agenda).text;
    assert!(
        !text.contains("ORGANIZER") && !text.contains("ATTENDEE"),
        "{text}"
    );
    let heard = agenda.heard.lines();
    assert!(
        heard.iter().all(|line| !line.starts_with("OPTIONS")),
        "{heard:?}"
    );
}

#[tokio::test]
async fn the_declared_agenda_itself_takes_the_event() {
    let behaviour = CalDavBehaviour::DeclaredIsAgenda;
    let (agenda, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let result = validated_call(&paul, CREATE, new_event()).await;
    assert!(!result.is_error, "{:?}", result.content);
    let id = result.content["id"].as_str().unwrap();
    assert_eq!(the_one(&agenda).path, format!("/dav/{id}.ics"));
    assert_eq!(result.content["agenda"], "Agenda");
}

#[tokio::test]
async fn the_attendees_are_invited_from_the_linked_address_or_else_the_server_s_first() {
    // The linked address among those the server knows the account by.
    static WITH_LINKED: [&str; 3] = [
        "/dav/principals/paul/",
        "mailto:direction@neo-maintenance.example",
        "MAILTO:PAUL.RENAUD@neo-maintenance.example",
    ];
    let behaviour = CalDavBehaviour::Addresses(&WITH_LINKED);
    let (agenda, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let result = validated_call(&paul, CREATE, new_event()).await;
    assert!(!result.is_error, "{:?}", result.content);
    let organizer = "PAUL.RENAUD@neo-maintenance.example";
    assert_eq!(result.content["organisateur"], organizer);
    assert!(
        the_one(&agenda)
            .text
            .contains(&format!("\r\nORGANIZER:mailto:{organizer}\r\n"))
    );
    // Otherwise the first it gives that an invitation writes as it is.
    static OTHERS: [&str; 3] = [
        "mailto:a?cc=tous@neo-maintenance.example",
        "mailto:secretariat@neo-maintenance.example",
        "mailto:direction@neo-maintenance.example",
    ];
    let behaviour = CalDavBehaviour::Addresses(&OTHERS);
    let (_, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let result = validated_call(&paul, CREATE, new_event()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(
        result.content["organisateur"],
        "secretariat@neo-maintenance.example"
    );
}
