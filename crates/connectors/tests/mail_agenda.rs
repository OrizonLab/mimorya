//! The standard mail's agenda (ADR-205): `mail_calendar_events`, offered
//! where the company declared a CalDAV agenda, reads the events of the
//! person the run serves over a period, with the account they linked, over
//! HTTPS: their principal, calendar home and agendas of events found on
//! the declared server, each occurrence asked expanded, the period's events
//! the earliest first, in Paris time, the period's bounds excluded as
//! RFC 4791 has it, a recurring event left whole kept and said so, a length
//! past any instant read safely. Nothing is sent to another server
//! (principal, calendar home or agenda), no redirect is followed, what the
//! server says is bounded, and a refused password, alone, removes the
//! account. A person linking an account finds an agenda with it
//! (`check_caldav`).

mod support;

use std::sync::Arc;
use std::time::Duration;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::outbound::TOO_LONG;
use mimorya_connectors::standard_mail::{
    ACCESS_REFUSED, CREDENTIALS_REFUSED, ELSEWHERE, REDIRECTED, TRY_LATER, check_caldav,
};
use mimorya_connectors::testing::{CalDavBehaviour, FakeCalDav, TestAuthority};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::{LinkedAccounts, MailServerRole};
use serde_json::{Value, json};
use support::linked_accounts::{call, error_of, mailbox_executor};
use support::mail::{PASSWORD, USER, WRONG, accounts, hides, login, trusting};
use support::mailboxes::linked_at;
use support::sending::tools;
use uuid::Uuid;

/// The company's declaration naming `agenda`.
fn declaration(agenda: &FakeCalDav) -> Value {
    json!({
        "imap": { "host": "127.0.0.1", "port": 993 },
        "smtp": { "host": "127.0.0.1", "port": 465, "security": "tls" },
        "caldav": agenda.base(),
    })
}

fn events() -> Vec<String> {
    [
        "BEGIN:VEVENT\r\nUID:visite@neo\r\nSUMMARY:Visite client\r\n\
         DTSTART;TZID=Europe/Paris:20261117T140000\r\nDTEND;TZID=Europe/Paris:20261117T150000\r\n\
         END:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:chantier@neo\r\nSUMMARY:Réunion de chantier\r\n\
         DTSTART:20261116T080000Z\r\nDTEND:20261116T090000Z\r\nLOCATION:Quai B\r\n\
         DESCRIPTION:Ordre du jour\\, sécurité\r\nORGANIZER;CN=Léa:mailto:lea@neo.example\r\n\
         ATTENDEE;CN=Paul:mailto:paul@neo.example\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:conges@neo\r\nSUMMARY:Congés\r\nDTSTART;VALUE=DATE:20261118\r\n\
         DTEND;VALUE=DATE:20261119\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nUID:plus-tard@neo\r\nSUMMARY:Hors période\r\n\
         DTSTART:20261220T080000Z\r\nDTEND:20261220T090000Z\r\nEND:VEVENT\r\n",
    ]
    .map(String::from)
    .to_vec()
}

/// Paul, his account linked with `password`, at an agenda behaving so.
async fn paul_at(
    behaviour: CalDavBehaviour,
    password: &str,
) -> (FakeCalDav, Arc<InMemoryPorts>, ToolExecutor) {
    paul_holding(behaviour, password, events()).await
}

/// The same, the agenda holding `held`.
async fn paul_holding(
    behaviour: CalDavBehaviour,
    password: &str,
    held: Vec<String>,
) -> (FakeCalDav, Arc<InMemoryPorts>, ToolExecutor) {
    let authority = TestAuthority::new();
    let agenda = FakeCalDav::start(&authority, accounts(), behaviour, held).await;
    let declared = declaration(&agenda);
    let ports = InMemoryPorts::new(user::PAUL, Uuid::new_v4());
    ports.link_mail_account(declared.clone(), USER, password, linked_at());
    let tools = tools(&authority, &declared, Duration::from_secs(20)).await;
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let paul = mailbox_executor(tools, user::PAUL, &ports, accounts);
    (agenda, ports, paul)
}

fn week() -> Value {
    json!({ "from": "2026-11-16", "to": "2026-11-18" })
}

#[tokio::test]
async fn the_agenda_is_offered_where_the_company_declared_one() {
    let authority = TestAuthority::new();
    let agenda = FakeCalDav::start(&authority, accounts(), CalDavBehaviour::Answers, vec![]).await;
    let mut declared = declaration(&agenda);
    declared["caldav"] = Value::Null;
    let without = tools(&authority, &declared, Duration::from_secs(5)).await;
    assert!(
        without
            .iter()
            .all(|t| t.info().name != "mail_calendar_events")
    );
    let with = tools(&authority, &declaration(&agenda), Duration::from_secs(5)).await;
    let tool = with
        .iter()
        .find(|t| t.info().name == "mail_calendar_events")
        .unwrap();
    let info = tool.info();
    assert_eq!(info.risk, ToolRisk::Read);
    assert_eq!(info.required_permissions, ["connector:use"]);
    assert!(!info.requires_approval);
}

#[tokio::test]
async fn the_period_s_events_come_earliest_first_in_paris_time() {
    let (agenda, _, paul) = paul_at(CalDavBehaviour::Answers, PASSWORD).await;
    let result = call(&paul, "mail_calendar_events", week()).await;
    assert!(!result.is_error, "{:?}", result.content);
    let read = &result.content["rendezVous"];
    let subjects: Vec<&str> = read
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event["objet"].as_str().unwrap())
        .collect();
    // The server answered every event: the one past the period is left.
    assert_eq!(subjects, ["Réunion de chantier", "Visite client", "Congés"]);
    let meeting = &read[0];
    assert_eq!(meeting["debut"], "2026-11-16T09:00:00+01:00");
    assert_eq!(meeting["fin"], "2026-11-16T10:00:00+01:00");
    assert_eq!(meeting["lieu"], "Quai B");
    assert_eq!(meeting["organisateur"], "Léa <lea@neo.example>");
    assert_eq!(meeting["participants"], json!(["Paul <paul@neo.example>"]));
    assert_eq!(meeting["apercu"], "Ordre du jour, sécurité");
    assert_eq!(meeting["agenda"], "Personnel");
    assert_eq!(read[1]["debut"], "2026-11-17T14:00:00+01:00");
    let leave = &read[2];
    assert_eq!(
        (&leave["debut"], &leave["fin"]),
        (&json!("2026-11-18"), &json!("2026-11-19"))
    );
    assert_eq!(
        (&leave["journeeEntiere"], &leave["annule"]),
        (&json!(true), &json!(true))
    );
    assert_eq!(result.citations[0].id, "chantier@neo");
    assert_eq!(result.summary.as_deref(), Some("3 rendez-vous"));
    // Its principal, its home and its agenda of events alone, expanded.
    let heard = agenda.heard.lines();
    assert_eq!(
        heard,
        [
            "PROPFIND /dav/",
            "PROPFIND /dav/principals/paul/",
            "PROPFIND /dav/calendars/paul/",
            "REPORT /dav/calendars/paul/personnel/ expand",
        ]
    );
}

#[tokio::test]
async fn nothing_goes_to_another_server_nor_follows_a_redirect() {
    let principal = CalDavBehaviour::PrincipalElsewhere("https://agenda.ailleurs.example/p/paul/");
    let home = CalDavBehaviour::HomeElsewhere("//agenda.ailleurs.example/dav/paul/");
    let refusals = [
        (principal, ELSEWHERE, 1),
        (home, ELSEWHERE, 2),
        (CalDavBehaviour::Redirects, REDIRECTED, 1),
        (CalDavBehaviour::AnswersAtLength, TOO_LONG, 4),
    ];
    for (behaviour, said, requests) in refusals {
        let (agenda, ports, paul) = paul_at(behaviour, PASSWORD).await;
        let error = error_of(&call(&paul, "mail_calendar_events", week()).await);
        assert!(error.starts_with(said), "{behaviour:?}: {error}");
        assert!(ports.refused_mail_accounts().is_empty());
        // Nothing past the answer naming another server.
        assert_eq!(agenda.heard.lines().len(), requests, "{behaviour:?}");
    }
    // Agendas listed on other servers (another host, in full or from the
    // scheme on, the same host on another port) are left: were any asked,
    // the call would fail.
    let (agenda, _, paul) = paul_at(CalDavBehaviour::ListsElsewhere, PASSWORD).await;
    let result = call(&paul, "mail_calendar_events", week()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 3);
    let reports: Vec<String> = agenda
        .heard
        .lines()
        .into_iter()
        .filter(|line| line.starts_with("REPORT"))
        .collect();
    assert_eq!(reports, ["REPORT /dav/calendars/paul/personnel/ expand"]);
}

#[tokio::test]
async fn a_refusal_other_than_the_password_keeps_the_account() {
    for (code, said) in [(403, ACCESS_REFUSED), (503, TRY_LATER)] {
        let (_, ports, paul) = paul_at(CalDavBehaviour::Refuses(code), PASSWORD).await;
        let error = error_of(&call(&paul, "mail_calendar_events", week()).await);
        assert!(error.starts_with(said), "{code}: {error}");
        assert!(ports.refused_mail_accounts().is_empty(), "{code}");
    }
}

#[tokio::test]
async fn the_period_s_bounds_lengths_and_recurrences_are_read_safely() {
    let held = [
        // Whole days just before and just after the period.
        "BEGIN:VEVENT\r\nSUMMARY:Veille\r\nDTSTART;VALUE=DATE:20261115\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nSUMMARY:Lendemain\r\nDTSTART;VALUE=DATE:20261119\r\nEND:VEVENT\r\n",
        // Ending at the period's start (midnight in Paris).
        "BEGIN:VEVENT\r\nSUMMARY:Finie avant\r\nDTSTART:20261115T220000Z\r\n\
         DTEND:20261115T230000Z\r\nEND:VEVENT\r\n",
        // A length past any instant, and a malformed one.
        "BEGIN:VEVENT\r\nSUMMARY:Sans fin\r\nDTSTART:20261116T080000Z\r\n\
         DURATION:P99999999D\r\nEND:VEVENT\r\n",
        "BEGIN:VEVENT\r\nSUMMARY:Durée illisible\r\nDTSTART:20261117T080000Z\r\n\
         DURATION:P106751991167D1D\r\nEND:VEVENT\r\n",
        // A weekly meeting begun earlier, which the server did not expand.
        "BEGIN:VEVENT\r\nSUMMARY:Point hebdomadaire\r\nDTSTART:20261102T090000Z\r\n\
         DTEND:20261102T100000Z\r\nRRULE:FREQ=WEEKLY\r\nEND:VEVENT\r\n",
    ]
    .map(String::from)
    .to_vec();
    let (_, _, paul) = paul_holding(CalDavBehaviour::Answers, PASSWORD, held).await;
    let result = call(&paul, "mail_calendar_events", week()).await;
    assert!(!result.is_error, "{:?}", result.content);
    let read = result.content["rendezVous"].as_array().unwrap().clone();
    let subjects: Vec<&str> = read.iter().map(|e| e["objet"].as_str().unwrap()).collect();
    assert_eq!(
        subjects,
        ["Point hebdomadaire", "Sans fin", "Durée illisible"]
    );
    assert!(read[0]["repetition"].as_str().unwrap().contains("répéter"));
    assert_eq!(read[1]["fin"], Value::Null);
    assert_eq!(read[2]["fin"], Value::Null);
}

#[tokio::test]
async fn a_refused_password_removes_the_account() {
    let (agenda, ports, paul) = paul_at(CalDavBehaviour::Answers, WRONG).await;
    let error = error_of(&call(&paul, "mail_calendar_events", week()).await);
    assert!(
        error.starts_with("Le serveur CalDAV de l’entreprise refuse le compte relié"),
        "{error}"
    );
    hides(&error, WRONG);
    assert_eq!(
        ports.mail_refusals(),
        [(linked_at(), MailServerRole::CalDav)]
    );
    let heard = agenda.heard.lines().len();
    let error = error_of(&call(&paul, "mail_calendar_events", week()).await);
    assert!(error.contains("n’a relié aucun compte"), "{error}");
    assert_eq!(agenda.heard.lines().len(), heard);
}

#[tokio::test]
async fn a_person_linking_an_account_finds_an_agenda_with_it() {
    let authority = TestAuthority::new();
    let agenda = FakeCalDav::start(&authority, accounts(), CalDavBehaviour::Answers, vec![]).await;
    let policy = trusting(&authority);
    check_caldav(&agenda.base(), &login(PASSWORD), policy)
        .await
        .unwrap();
    let refused = check_caldav(&agenda.base(), &login(WRONG), policy)
        .await
        .unwrap_err();
    assert_eq!(refused.summary, CREDENTIALS_REFUSED);
    hides(&refused.detail, WRONG);
    // Another authority's certificate is never trusted.
    let stranger = TestAuthority::new();
    let untrusted = check_caldav(&agenda.base(), &login(PASSWORD), trusting(&stranger)).await;
    assert!(untrusted.is_err());
}
