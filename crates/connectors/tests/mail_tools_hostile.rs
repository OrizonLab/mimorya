//! The standard mail's reading tools against a hostile server or sender
//! (ADR-205): what the server sends unasked or without end, a structure
//! nested without end, an e-mail enclosing e-mails without end however it
//! sends them. Each is left or refused, the account kept, the process
//! running on.

mod support;

use mimorya_connectors::outbound::TOO_LONG;
use mimorya_connectors::standard_mail::NOT_A_MAIL_SERVER;
use mimorya_connectors::testing::{
    FakeImap, FetchBehaviour, ImapBehaviour, ImapSetup, TestAuthority,
};
use mimorya_core::demo::user;
use serde_json::json;
use support::linked_accounts::{call, error_of};
use support::mail::{PASSWORD, USER, accounts, trusting};
use support::mailboxes::{
    Enclosed, VALIDITY, declaration, enclosing_without_end, folders, linked_at, linked_paul,
    started_on,
};

#[tokio::test]
async fn what_the_server_sends_unasked_or_without_end_is_left() {
    let authority = TestAuthority::new();
    let mut adding = folders(None);
    adding.fetch = FetchBehaviour::AddsTheFirst;
    let (_, _, paul) = linked_paul(&authority, adding).await;
    let result = call(&paul, "mail_search", json!({ "query": "relance" })).await;
    let ids: Vec<&str> = result.content["emails"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [format!("i{VALIDITY}.12")]);
    // Asked, then sent once more unasked: shown once.
    let result = call(&paul, "mail_search", json!({ "query": "facture" })).await;
    let ids: Vec<&str> = result.content["emails"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [format!("i{VALIDITY}.12"), format!("i{VALIDITY}.3")]);

    let mut endless = folders(None);
    endless.fetch = FetchBehaviour::Endless;
    let (_, _, paul) = linked_paul(&authority, endless).await;
    let result = call(
        &paul,
        "mail_read",
        json!({ "id": format!("i{VALIDITY}.8") }),
    )
    .await;
    assert!(
        error_of(&result).starts_with(TOO_LONG),
        "{:?}",
        result.content
    );
}

#[tokio::test]
async fn a_structure_nested_without_end_is_refused_the_account_kept() {
    let authority = TestAuthority::new();
    let mut nested = folders(None);
    nested.fetch = FetchBehaviour::Nested;
    let imap = FakeImap::holding(
        &authority,
        accounts(),
        ImapSetup::new(ImapBehaviour::Answers),
        nested,
    )
    .await;
    let ports = mimorya_agent::testing::InMemoryPorts::new(user::PAUL, uuid::Uuid::new_v4());
    ports.link_mail_account(declaration(imap.address), USER, PASSWORD, linked_at());
    let paul = started_on(&declaration(imap.address), trusting(&authority), &ports).await;
    // Read by a parser calling itself for each parenthesis, a thousand of
    // them would end the process: refused, the process goes on.
    let refused = call(&paul, "mail_search", json!({ "query": "facture" })).await;
    assert!(
        error_of(&refused).starts_with(NOT_A_MAIL_SERVER),
        "{:?}",
        refused.content
    );
    assert!(ports.refused_mail_accounts().is_empty());
}

#[tokio::test]
async fn an_e_mail_enclosing_e_mails_without_end_is_read_not_the_process_ended() {
    let authority = TestAuthority::new();
    let mut mailboxes = folders(None);
    let forms = [
        Enclosed::AsTheyAre,
        Enclosed::QuotedPrintable,
        Enclosed::Base64,
    ];
    for (uid, form) in (20..).zip(forms) {
        mailboxes.folders[0]
            .messages
            .push(enclosing_without_end(uid, form));
    }
    let (_, _, paul) = linked_paul(&authority, mailboxes).await;
    // Read or let go of one within another, they would overflow this
    // thread's stack.
    for uid in 20..23 {
        let id = format!("i{VALIDITY}.{uid}");
        let result = call(&paul, "mail_read", json!({ "id": id })).await;
        assert!(!result.is_error, "{uid}: {:?}", result.content);
        assert_eq!(result.content["objet"], "Emboîtés");
    }
}
