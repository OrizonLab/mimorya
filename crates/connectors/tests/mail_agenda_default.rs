//! Where `mail_calendar_event_create` (ADR-205) puts a new event when the
//! person has more than one agenda taking events: in the one their
//! server names for new events (RFC 6638: `schedule-default-calendar-URL`
//! of the Inbox their principal names), when it is listed as taking them;
//! else in the first that does. A default that is read-only, unlisted or
//! on another server, and an Inbox elsewhere or silent, leave the first;
//! nothing is asked of another server. With a single agenda taking events,
//! no default is asked (`mail_agenda_create.rs`).

mod support;

use std::time::Duration;

use mimorya_connectors::testing::{AGENDA_PATH, CalDavBehaviour, INBOX_PATH, WORK_PATH};
use support::agenda::{new_event, paul_beside_another, paul_writing_at};
use support::linked_accounts::validated_call;

const CREATE: &str = "mail_calendar_event_create";

#[tokio::test]
async fn the_event_goes_in_the_agenda_the_server_names_for_new_events() {
    let behaviour = CalDavBehaviour::DefaultAgenda(INBOX_PATH, WORK_PATH);
    let (agenda, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let result = validated_call(&paul, CREATE, new_event()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["agenda"], "Travail");
    let id = result.content["id"].as_str().unwrap();
    let created = agenda.created.all();
    assert_eq!(created.len(), 1, "{created:?}");
    assert_eq!(created[0].path, format!("{WORK_PATH}{id}.ics"));
    // Once the agendas are listed: the principal's Inbox, the agenda it
    // names, then the invitations asked of that agenda.
    let heard = agenda.heard.lines();
    let listed = heard
        .iter()
        .position(|line| line == "PROPFIND /dav/calendars/paul/")
        .unwrap();
    assert_eq!(
        heard[listed + 1..],
        [
            "PROPFIND /dav/principals/paul/".to_string(),
            format!("PROPFIND {INBOX_PATH}"),
            format!("OPTIONS {WORK_PATH}"),
            "PROPFIND /dav/principals/paul/".into(),
            format!("PUT {WORK_PATH}{id}.ics"),
        ]
    );
}

#[tokio::test]
async fn a_default_it_may_not_take_leaves_the_first_agenda() {
    for default in [
        // Of tasks: not listed as taking events.
        "/dav/calendars/paul/taches/",
        // Not listed at all.
        "/dav/calendars/paul/absent/",
        // An Inbox that does not answer.
        "",
    ] {
        let behaviour = match default {
            "" => CalDavBehaviour::DefaultAgenda("/dav/inbox/absent/", WORK_PATH),
            named => CalDavBehaviour::DefaultAgenda(INBOX_PATH, named),
        };
        let (agenda, _, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
        let result = validated_call(&paul, CREATE, new_event()).await;
        assert!(!result.is_error, "{default}: {:?}", result.content);
        assert_eq!(result.content["agenda"], "Personnel", "{default}");
        let id = result.content["id"].as_str().unwrap();
        let created = agenda.created.all();
        assert_eq!(created.len(), 1, "{default}: {created:?}");
        assert_eq!(created[0].path, format!("{AGENDA_PATH}{id}.ics"));
    }
}

#[tokio::test]
async fn nothing_is_asked_of_an_inbox_or_a_default_on_another_server() {
    for names in [
        (|elsewhere| CalDavBehaviour::DefaultAgenda(elsewhere, WORK_PATH))
            as fn(&'static str) -> CalDavBehaviour,
        |elsewhere| CalDavBehaviour::DefaultAgenda(INBOX_PATH, elsewhere),
    ] {
        let (agenda, elsewhere, paul) = paul_beside_another(names).await;
        let result = validated_call(&paul, CREATE, new_event()).await;
        assert!(!result.is_error, "{:?}", result.content);
        assert_eq!(result.content["agenda"], "Personnel");
        let id = result.content["id"].as_str().unwrap();
        assert_eq!(
            agenda.created.all()[0].path,
            format!("{AGENDA_PATH}{id}.ics")
        );
        assert!(
            elsewhere.heard.lines().is_empty(),
            "{:?}",
            elsewhere.heard.lines()
        );
        assert!(elsewhere.created.all().is_empty());
    }
}
