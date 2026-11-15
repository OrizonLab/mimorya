//! What `mail_calendar_event_create` (ADR-205) says when an event is not
//! known to be created: nothing created when the server would not invite
//! the attendees, takes no event in any agenda of the account, refuses it,
//! or is silent before it, or an attendee's address would be read as
//! another in an invitation; perhaps created when the server fails or is
//! silent once it was sent. A principal the server names on another
//! server hears nothing of the account. A refused password, alone, removes
//! the account.

mod support;

use std::time::Duration;

use mimorya_connectors::standard_mail::{
    ACCESS_REFUSED, EVENT_REFUSED, EVENT_UNSURE, NO_ORGANIZER, NO_SCHEDULING, NO_WRITABLE_AGENDA,
    TRY_LATER,
};
use mimorya_connectors::testing::CalDavBehaviour;
use mimorya_core::tools::MailServerRole;
use serde_json::json;
use support::agenda::{new_event, paul_beside_another, paul_writing_at};
use support::linked_accounts::{error_of, validated_call};
use support::mailboxes::linked_at;

const CREATE: &str = "mail_calendar_event_create";
const NOTHING: &str = "— rien n’est créé";
const PERHAPS: &str = "il est peut-être créé, vérifiez l’agenda avant de le créer de nouveau";

/// What Paul is told creating the event at an agenda behaving so, each
/// call within `call_timeout`; and whether the server kept anything and
/// heard a `PUT`.
async fn told(behaviour: CalDavBehaviour, call_timeout: Duration) -> (String, usize, bool) {
    let (agenda, _, paul) = paul_writing_at(behaviour, call_timeout).await;
    let error = error_of(&validated_call(&paul, CREATE, new_event()).await);
    let put = agenda.heard.lines().iter().any(|l| l.starts_with("PUT "));
    (error, agenda.created.all().len(), put)
}

#[tokio::test]
async fn attendees_the_server_would_not_invite_leave_nothing_created() {
    let (error, kept, put) = told(CalDavBehaviour::NoScheduling, Duration::from_secs(20)).await;
    assert!(error.starts_with(NO_SCHEDULING), "{error}");
    assert!(
        error.contains("créez-le sans participant") && error.ends_with(NOTHING),
        "{error}"
    );
    assert_eq!((kept, put), (0, false));
    // No address of the account an invitation could be sent from.
    static NONE: [&str; 2] = [
        "/dav/principals/paul/",
        "mailto:a#b@neo-maintenance.example",
    ];
    let behaviour = CalDavBehaviour::Addresses(&NONE);
    let (error, kept, put) = told(behaviour, Duration::from_secs(20)).await;
    assert!(
        error.starts_with(NO_ORGANIZER) && error.ends_with(NOTHING),
        "{error}"
    );
    assert_eq!((kept, put), (0, false));
}

#[tokio::test]
async fn a_principal_on_another_server_hears_nothing_of_the_account() {
    // Beside a calendar home of its own, or at a root that is an agenda:
    // the search goes on, the principal is never asked whom to invite as.
    let names: [fn(&'static str) -> CalDavBehaviour; 2] = [
        CalDavBehaviour::HomeBesidePrincipalElsewhere,
        CalDavBehaviour::AgendaWithPrincipalElsewhere,
    ];
    for named in names {
        let (agenda, elsewhere, paul) = paul_beside_another(named).await;
        let error = error_of(&validated_call(&paul, CREATE, new_event()).await);
        assert!(
            error.starts_with(NO_ORGANIZER) && error.ends_with(NOTHING),
            "{error}"
        );
        assert!(
            elsewhere.heard.lines().is_empty(),
            "{:?}",
            elsewhere.heard.lines()
        );
        assert!(agenda.created.all().is_empty());
        assert!(
            agenda
                .heard
                .lines()
                .iter()
                .any(|l| l.starts_with("OPTIONS"))
        );
    }
}

#[tokio::test]
async fn an_account_that_only_reads_its_agendas_creates_nothing() {
    let (error, kept, put) = told(CalDavBehaviour::ReadOnly, Duration::from_secs(20)).await;
    assert!(
        error.starts_with(NO_WRITABLE_AGENDA) && error.ends_with(NOTHING),
        "{error}"
    );
    assert_eq!((kept, put), (0, false));
}

#[tokio::test]
async fn an_attendee_an_invitation_would_read_otherwise_reaches_no_server() {
    let (agenda, _, paul) =
        paul_writing_at(CalDavBehaviour::Answers, Duration::from_secs(20)).await;
    let mut event = new_event();
    event["attendees"] = json!(["claire@exemple.fr", "a?cc=tous@exemple.fr"]);
    let error = error_of(&validated_call(&paul, CREATE, event).await);
    assert!(
        error.starts_with("L’adresse de participant a?cc=tous@exemple.fr ne s’écrit pas"),
        "{error}"
    );
    assert!(error.ends_with(NOTHING), "{error}");
    assert!(agenda.heard.lines().is_empty());
}

#[tokio::test]
async fn an_event_the_server_refuses_is_not_created() {
    for (code, said) in [
        (403, ACCESS_REFUSED),
        (412, EVENT_REFUSED),
        (415, EVENT_REFUSED),
        (503, TRY_LATER),
    ] {
        let behaviour = CalDavBehaviour::PutRefuses(code);
        let (error, kept, put) = told(behaviour, Duration::from_secs(20)).await;
        assert!(error.starts_with(said), "{code}: {error}");
        assert!(error.ends_with(NOTHING), "{code}: {error}");
        assert_eq!((kept, put), (0, true), "{code}");
    }
}

#[tokio::test]
async fn a_server_failing_or_silent_once_the_event_is_sent_may_have_created_it() {
    let (error, _, put) = told(CalDavBehaviour::PutRefuses(502), Duration::from_secs(20)).await;
    assert!(
        error.starts_with(EVENT_UNSURE) && error.contains(PERHAPS),
        "{error}"
    );
    assert!(put);
    let (error, kept, _) = told(CalDavBehaviour::PutHangs, Duration::from_secs(2)).await;
    assert!(error.contains(PERHAPS), "{error}");
    assert_eq!(kept, 1);
    // Silent before: nothing was sent.
    let (error, kept, put) = told(CalDavBehaviour::Hangs, Duration::from_secs(2)).await;
    assert!(error.ends_with(NOTHING), "{error}");
    assert_eq!((kept, put), (0, false));
}

#[tokio::test]
async fn a_refused_password_removes_the_account_a_refused_event_keeps_it() {
    let behaviour = CalDavBehaviour::PutRefuses(401);
    let (agenda, ports, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let error = error_of(&validated_call(&paul, CREATE, new_event()).await);
    assert!(
        error.starts_with("Le serveur CalDAV de l’entreprise refuse le compte relié"),
        "{error}"
    );
    assert_eq!(
        ports.mail_refusals(),
        [(linked_at(), MailServerRole::CalDav)]
    );
    assert!(agenda.created.all().is_empty());
    let behaviour = CalDavBehaviour::PutRefuses(403);
    let (_, ports, paul) = paul_writing_at(behaviour, Duration::from_secs(20)).await;
    let _ = validated_call(&paul, CREATE, new_event()).await;
    assert!(ports.refused_mail_accounts().is_empty());
}
