//! The standard mail's agenda (ADR-205): `mail_calendar_events`, offered
//! where the company declared a CalDAV agenda, reads the events of the
//! person the run serves over a period, with the account they linked, over
//! HTTPS: their principal, calendar home and agendas of events found on
//! the declared server, each occurrence asked expanded, the period's events
//! the earliest first, in Paris time; how they are read is tried in
//! `mail_agenda_reading.rs`. Nothing is sent to another server
//! (principal, calendar home or agenda), no redirect is followed, what the
//! server says is bounded, and a refused password, alone, removes the
//! account. A person linking an account finds an agenda with it
//! (`check_caldav`).

mod support;

use std::time::Duration;

use mimorya_connectors::outbound::TOO_LONG;
use mimorya_connectors::standard_mail::{
    ACCESS_REFUSED, CREDENTIALS_REFUSED, ELSEWHERE, NO_AGENDA, REDIRECTED, TRY_LATER, check_caldav,
};
use mimorya_connectors::testing::{CalDavBehaviour, FakeCalDav, TestAuthority};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::tools::MailServerRole;
use serde_json::{Value, json};
use support::agenda::{declaration, paul_at, week};
use support::linked_accounts::{call, error_of};
use support::mail::{PASSWORD, WRONG, accounts, hides, login, trusting};
use support::mailboxes::linked_at;
use support::sending::tools;

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
    assert_eq!(read[0]["occurrence"], Value::Null);
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
    let named = CalDavBehaviour::DeclaredNamesHome("https://agenda.ailleurs.example/dav/paul/");
    let refusals = [
        (principal, ELSEWHERE, 1),
        (home, ELSEWHERE, 2),
        (named, ELSEWHERE, 1),
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
async fn an_account_with_no_agenda_of_events_is_told_so() {
    let (_, ports, paul) = paul_at(CalDavBehaviour::TasksOnly, PASSWORD).await;
    let error = error_of(&call(&paul, "mail_calendar_events", week()).await);
    assert!(error.starts_with(NO_AGENDA), "{error}");
    assert!(ports.refused_mail_accounts().is_empty());
    // Nor is such an account linked.
    let authority = TestAuthority::new();
    let behaviour = CalDavBehaviour::TasksOnly;
    let agenda = FakeCalDav::start(&authority, accounts(), behaviour, vec![]).await;
    let refused = check_caldav(&agenda.base(), &login(PASSWORD), trusting(&authority))
        .await
        .unwrap_err();
    assert_eq!(refused.summary, NO_AGENDA);
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
