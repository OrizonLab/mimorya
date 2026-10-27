//! The Microsoft 365 tools reach the account of the person the run serves
//! only (ADR-117): the calendar read with that person's token, and what
//! the model is told when there is none, or Graph refuses it.

mod support;

use std::sync::Arc;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_connectors::testing::FakeGraph;
use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::demo::user;
use mimorya_core::tools::{AccountAccessError, LinkedAccounts};
use serde_json::json;
use support::microsoft365::{Unreachable, call, error_of, mailbox_executor, microsoft365_tools};
use uuid::Uuid;

fn ports_of(user_id: Uuid, token: Option<&str>) -> Arc<InMemoryPorts> {
    let ports = InMemoryPorts::new(user_id, Uuid::new_v4());
    if let Some(token) = token {
        ports.link_account(MICROSOFT365, token);
    }
    ports
}

#[tokio::test]
async fn the_calendar_is_read_over_days_of_the_platforms_zone() {
    let graph = FakeGraph::start().await;
    let ports = ports_of(user::LEA, Some("acces-lea-1"));
    let executor = mailbox_executor(
        microsoft365_tools(&graph.url).await,
        user::LEA,
        &ports,
        ports.clone(),
    );
    let week = json!({ "from": "2026-10-26", "to": "2026-10-30" });
    let result = call(&executor, "m365_calendar_events", week).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 2);
    let first = &result.content["rendezVous"][0];
    assert_eq!(first["objet"], "Point d’équipe — lea");
    assert_eq!(first["lieu"], "Salle Atlas");
    assert_eq!(
        first["participants"],
        json!(["Léa Martin <lea@exemple.fr>"])
    );
    assert_eq!(result.citations[1].kind, "event");
    let asked = &graph.requests()[0];
    assert_eq!(asked.path, "/me/calendarView");
    // Paris' days: winter time from the 25th of October.
    assert_eq!(asked.query["startDateTime"], "2026-10-25T23:00:00Z");
    assert_eq!(asked.query["endDateTime"], "2026-10-30T23:00:00Z");
    assert_eq!(asked.query["$orderby"], "start/dateTime");
    assert_eq!(
        asked.prefer.as_deref(),
        Some("outlook.timezone=\"Europe/Paris\"")
    );
    let long = json!({ "from": "2026-10-01", "to": "2026-11-01" });
    let error = error_of(&call(&executor, "m365_calendar_events", long).await);
    assert!(error.contains("31 jours"), "{error}");
    assert_eq!(graph.requests().len(), 1);
}

#[tokio::test]
async fn each_run_reads_its_own_persons_mailbox() {
    let graph = FakeGraph::start().await;
    let tools = microsoft365_tools(&graph.url).await;
    for (user_id, owner) in [(user::LEA, "lea"), (user::MARC, "marc")] {
        let ports = ports_of(user_id, Some(&format!("acces-{owner}-1")));
        let executor = mailbox_executor(tools.clone(), user_id, &ports, ports.clone());
        let result = call(&executor, "m365_mail_search", json!({})).await;
        for email in result.content["emails"].as_array().unwrap() {
            assert!(email["objet"].as_str().unwrap().ends_with(owner), "{email}");
        }
    }
}

#[tokio::test]
async fn without_a_linked_account_nothing_reaches_graph() {
    let graph = FakeGraph::start().await;
    let tools = microsoft365_tools(&graph.url).await;
    let ports = ports_of(user::LEA, None);
    let executor = mailbox_executor(tools.clone(), user::LEA, &ports, ports.clone());
    let error = error_of(&call(&executor, "m365_mail_search", json!({})).await);
    assert!(error.contains("« Mon compte »"), "{error}");
    let revoked = Arc::new(Unreachable(AccountAccessError::Revoked));
    let executor = mailbox_executor(tools.clone(), user::LEA, &ports, revoked);
    let error = error_of(&call(&executor, "m365_mail_read", json!({ "id": "x" })).await);
    assert!(error.contains("relie de nouveau"), "{error}");
    let why = AccountAccessError::Unavailable("Hors service pour l’instant".into());
    let executor = mailbox_executor(tools, user::LEA, &ports, Arc::new(Unreachable(why)));
    let week = json!({ "from": "2026-10-26", "to": "2026-10-30" });
    let error = error_of(&call(&executor, "m365_calendar_events", week).await);
    assert_eq!(error, "Hors service pour l’instant");
    assert!(graph.requests().is_empty());
}

#[tokio::test]
async fn a_token_graph_refuses_is_reported_to_the_platform() {
    let graph = FakeGraph::start().await;
    graph.refuse("acces-lea-1");
    let ports = ports_of(user::LEA, Some("acces-lea-1"));
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    let executor = mailbox_executor(
        microsoft365_tools(&graph.url).await,
        user::LEA,
        &ports,
        accounts,
    );
    let error = error_of(&call(&executor, "m365_mail_search", json!({})).await);
    assert!(error.contains("réessayez"), "{error}");
    assert_eq!(ports.rejected_accounts(), [MICROSOFT365]);
    // Another refusal says nothing of the token.
    let ok = ports_of(user::LEA, Some("acces-lea-2"));
    let executor = mailbox_executor(
        microsoft365_tools(&graph.url).await,
        user::LEA,
        &ok,
        ok.clone(),
    );
    call(&executor, "m365_mail_read", json!({ "id": "interdit" })).await;
    assert!(ok.rejected_accounts().is_empty());
}
