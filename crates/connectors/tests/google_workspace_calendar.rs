//! The Google Calendar tool (ADR-122): the events of the person the run
//! serves only, over days of the platform's zone, each occurrence apart;
//! and what the model is told when the person linked no account.

mod support;

use std::sync::Arc;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_connectors::testing::FakeGoogle;
use mimorya_core::account_links::GOOGLE_WORKSPACE;
use mimorya_core::demo::user;
use mimorya_core::tools::AccountAccessError;
use serde_json::json;
use support::google_workspace::google_tools;
use support::linked_accounts::{Unreachable, call, error_of, mailbox_executor};
use uuid::Uuid;

#[tokio::test]
async fn the_calendar_is_read_over_days_of_the_platforms_zone() {
    let google = FakeGoogle::start().await;
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account(GOOGLE_WORKSPACE, "acces-lea-1");
    let executor = mailbox_executor(
        google_tools(&google).await,
        user::LEA,
        &ports,
        ports.clone(),
    );
    let week = json!({ "from": "2026-10-26", "to": "2026-10-30" });
    let result = call(&executor, "google_calendar_events", week).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 2);
    let first = &result.content["rendezVous"][0];
    assert_eq!(first["objet"], "Point d’équipe — lea");
    assert_eq!(first["lieu"], "Salle Atlas");
    assert_eq!(first["organisateur"], "lea <lea@exemple.fr>");
    assert_eq!(
        first["participants"],
        json!(["Léa Martin <lea@exemple.fr>"])
    );
    assert_eq!(first["apercu"], "Ordre du jour");
    let second = &result.content["rendezVous"][1];
    assert_eq!(second["debut"], "2026-10-28");
    assert_eq!(second["journeeEntiere"], true);
    assert_eq!(result.citations[1].kind, "event");
    let asked = &google.requests()[0];
    assert_eq!(asked.path, "/calendars/primary/events");
    // Paris' days: winter time from the 25th of October.
    assert_eq!(asked.param("timeMin"), Some("2026-10-25T23:00:00Z"));
    assert_eq!(asked.param("timeMax"), Some("2026-10-30T23:00:00Z"));
    assert_eq!(asked.param("singleEvents"), Some("true"));
    assert_eq!(asked.param("orderBy"), Some("startTime"));
    assert_eq!(asked.param("timeZone"), Some("Europe/Paris"));
    let long = json!({ "from": "2026-10-01", "to": "2026-11-01" });
    let error = error_of(&call(&executor, "google_calendar_events", long).await);
    assert!(error.contains("31 jours"), "{error}");
    assert_eq!(google.requests().len(), 1);
}

#[tokio::test]
async fn without_a_linked_account_nothing_reaches_google() {
    let google = FakeGoogle::start().await;
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    for (error, said) in [
        (
            AccountAccessError::NotLinked,
            "Aucun compte Google Workspace relié",
        ),
        (
            AccountAccessError::Revoked,
            "Google Workspace n’honore plus",
        ),
    ] {
        let executor = mailbox_executor(
            google_tools(&google).await,
            user::LEA,
            &ports,
            Arc::new(Unreachable(error)),
        );
        let week = json!({ "from": "2026-10-26", "to": "2026-10-30" });
        let told = error_of(&call(&executor, "google_calendar_events", week).await);
        assert!(told.starts_with(said), "{told}");
    }
    assert!(google.requests().is_empty());
}
