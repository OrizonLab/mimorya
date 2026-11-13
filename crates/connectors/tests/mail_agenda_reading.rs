//! How the standard mail's agenda is read (ADR-205): the person's agendas
//! found at the declared address, by the calendar home it or the principal
//! names, or at the address itself when neither names one, ten at most;
//! the period's bounds excluded as RFC 4791 has it, a recurring event left
//! whole kept and said so, a length past any instant read safely; each
//! occurrence named, and what the answer leaves counted.

mod support;

use mimorya_connectors::testing::CalDavBehaviour;
use serde_json::Value;
use support::agenda::{paul_at, paul_holding, week};
use support::linked_accounts::call;
use support::mail::PASSWORD;

#[tokio::test]
async fn an_agenda_at_the_declared_address_or_a_home_it_names_is_read_there() {
    let cases = [
        // Neither the address nor the principal names a home: the address
        // is listed itself.
        (
            CalDavBehaviour::RootOnly,
            vec![
                "PROPFIND /dav/",
                "PROPFIND /dav/",
                "REPORT /dav/calendars/paul/personnel/ expand",
            ],
        ),
        (
            CalDavBehaviour::PrincipalWithoutHome,
            vec![
                "PROPFIND /dav/",
                "PROPFIND /dav/principals/paul/",
                "PROPFIND /dav/",
                "REPORT /dav/calendars/paul/personnel/ expand",
            ],
        ),
        (
            CalDavBehaviour::DeclaredIsAgenda,
            vec!["PROPFIND /dav/", "REPORT /dav/ expand"],
        ),
        (
            CalDavBehaviour::DeclaredNamesHome("/dav/calendars/paul/"),
            vec![
                "PROPFIND /dav/",
                "PROPFIND /dav/calendars/paul/",
                "REPORT /dav/calendars/paul/personnel/ expand",
            ],
        ),
    ];
    for (behaviour, asked) in cases {
        let (agenda, _, paul) = paul_at(behaviour, PASSWORD).await;
        let result = call(&paul, "mail_calendar_events", week()).await;
        assert!(!result.is_error, "{behaviour:?}: {:?}", result.content);
        assert_eq!(result.content["nombre"], 3, "{behaviour:?}");
        assert_eq!(agenda.heard.lines(), asked, "{behaviour:?}");
    }
}

#[tokio::test]
async fn ten_agendas_are_read_at_most() {
    let (agenda, _, paul) = paul_at(CalDavBehaviour::ManyAgendas, PASSWORD).await;
    let result = call(&paul, "mail_calendar_events", week()).await;
    assert!(!result.is_error, "{:?}", result.content);
    let reports = agenda
        .heard
        .lines()
        .iter()
        .filter(|line| line.starts_with("REPORT"))
        .count();
    assert_eq!(reports, 10);
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
async fn the_answer_names_occurrences_and_counts_what_it_leaves() {
    let mut held = vec![
        // An occurrence the server expanded.
        "BEGIN:VEVENT\r\nUID:hebdo@neo\r\nSUMMARY:Point\r\nDTSTART:20261116T060000Z\r\n\
         DTEND:20261116T063000Z\r\nRECURRENCE-ID:20261116T060000Z\r\nEND:VEVENT\r\n"
            .to_string(),
    ];
    let attendees: String = (0..11)
        .map(|at| format!("ATTENDEE:mailto:invite{at}@neo.example\r\n"))
        .collect();
    held.push(format!(
        "BEGIN:VEVENT\r\nUID:assemblee@neo\r\nSUMMARY:Assemblée\r\nDTSTART:20261116T070000Z\r\n\
         DTEND:20261116T080000Z\r\n{attendees}END:VEVENT\r\n"
    ));
    for minute in 0..50 {
        held.push(format!(
            "BEGIN:VEVENT\r\nUID:r{minute}@neo\r\nSUMMARY:Rendez-vous {minute}\r\n\
             DTSTART:20261117T10{minute:02}00Z\r\nEND:VEVENT\r\n"
        ));
    }
    let (_, _, paul) = paul_holding(CalDavBehaviour::Answers, PASSWORD, held).await;
    let result = call(&paul, "mail_calendar_events", week()).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 50);
    let left = result.content["autres"].as_str().unwrap();
    assert!(left.starts_with("2 autre(s)"), "{left}");
    let read = result.content["rendezVous"].as_array().unwrap();
    assert_eq!(read[0]["occurrence"], "20261116T060000Z");
    assert_eq!(read[1]["participants"].as_array().unwrap().len(), 10);
    assert_eq!(read[1]["autresParticipants"], 1);
    assert_eq!(read[2]["occurrence"], Value::Null);
}
