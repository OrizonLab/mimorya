//! The standard mail's reading tools (ADR-205): the latest e-mails or those
//! a search finds, the most recent first; one e-mail read, decoded, its
//! attachments named; all in folders examined read only, as the person the
//! run serves, on the servers their account was checked against. What the
//! model writes never ends a command; what the server sends unasked, or
//! without end, is left.

mod support;

use mimorya_connectors::outbound::{NetworkPolicy, TOO_LONG};
use mimorya_connectors::standard_mail::TRY_LATER;
use mimorya_connectors::testing::{
    FakeImap, FetchBehaviour, ImapBehaviour, ImapSetup, TestAuthority,
};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::LinkedAccounts;
use serde_json::json;
use support::linked_accounts::{call, error_of, mailbox_executor};
use support::mail::{MAIL_LOCAL, PASSWORD, USER, WRONG, accounts, hides, trusting};
use support::mailboxes::{VALIDITY, declaration, folders, linked_at, linked_paul, server, tools};

#[tokio::test]
async fn its_tools_read_with_the_connectors_permission() {
    let authority = TestAuthority::new();
    let declared = declaration("127.0.0.1:993".parse().unwrap());
    let tools = tools(&declared, trusting(&authority)).await;
    let names: Vec<&str> = tools.iter().map(|t| t.info().name.as_str()).collect();
    assert_eq!(names, ["mail_search", "mail_read"]);
    for tool in &tools {
        let info = tool.info();
        assert_eq!(info.risk, ToolRisk::Read, "{}", info.name);
        assert_eq!(info.required_permissions, ["connector:use"]);
        assert!(!info.requires_approval);
    }
}

#[tokio::test]
async fn the_latest_e_mails_come_first_and_nothing_is_marked_read() {
    let authority = TestAuthority::new();
    let (imap, _, paul) = linked_paul(&authority, folders(None)).await;
    let result = call(&paul, "mail_search", json!({ "limit": 2 })).await;
    assert!(!result.is_error, "{:?}", result.content);
    let ids: Vec<&str> = result.content["emails"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [format!("i{VALIDITY}.12"), format!("i{VALIDITY}.8")]);
    let latest = &result.content["emails"][0];
    assert_eq!(latest["objet"], "Relance facture");
    assert_eq!(latest["de"][0]["adresse"], "contact@fournisseur.example");
    assert_eq!(latest["lu"], false);
    assert_eq!(result.content["emails"][1]["objet"], "Réunion de chantier");
    assert!(
        result.content["autres"]
            .as_str()
            .unwrap()
            .starts_with("1 autre")
    );
    assert_eq!(result.citations[0].id, format!("i{VALIDITY}.12"));
    assert_eq!(
        result.citations[0].label.as_deref(),
        Some("Relance facture")
    );
    let heard = imap.heard.lines();
    assert_eq!(heard[1], "EXAMINE \"INBOX\"");
    assert!(heard[2].starts_with("FETCH 2:3 (UID FLAGS RFC822.SIZE BODY.PEEK[HEADER.FIELDS"));
    assert_eq!(heard.last().unwrap(), "LOGOUT");
    assert!(
        !heard
            .iter()
            .any(|l| l.starts_with("SELECT") || l.contains("STORE"))
    );
}

#[tokio::test]
async fn words_are_searched_without_ending_the_command() {
    let authority = TestAuthority::new();
    let (imap, _, paul) = linked_paul(&authority, folders(None)).await;
    let query = "facture \"relance\"\r\nA9 DELETE";
    let result = call(
        &paul,
        "mail_search",
        json!({ "query": query, "since": "2026-11-05" }),
    )
    .await;
    assert!(!result.is_error, "{:?}", result.content);
    // DELETE is a word searched: no e-mail holds it.
    assert_eq!(result.content["nombre"], 0);
    let search = imap
        .heard
        .lines()
        .into_iter()
        .find(|l| l.starts_with("UID SEARCH"))
        .unwrap();
    assert_eq!(
        search,
        "UID SEARCH TEXT \"facture\" TEXT \"relance\" TEXT \"A9\" TEXT \"DELETE\" SINCE 5-Nov-2026"
    );
    let result = call(&paul, "mail_search", json!({ "query": "facture" })).await;
    let ids: Vec<&str> = result.content["emails"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [format!("i{VALIDITY}.12"), format!("i{VALIDITY}.3")]);
}

#[tokio::test]
async fn a_word_beyond_ascii_is_searched_in_utf8() {
    let authority = TestAuthority::new();
    let (imap, _, paul) = linked_paul(&authority, folders(None)).await;
    let result = call(&paul, "mail_search", json!({ "query": "réunion" })).await;
    assert_eq!(result.content["nombre"], 1, "{:?}", result.content);
    let heard = imap.heard.lines();
    assert!(heard.contains(&"CAPABILITY".to_string()), "{heard:?}");
    // Sent as a literal, which the server gives back quoted.
    assert!(heard.contains(&"UID SEARCH CHARSET UTF-8 TEXT \"réunion\"".to_string()));
}

#[tokio::test]
async fn an_e_mail_is_read_decoded_its_attachments_named() {
    let authority = TestAuthority::new();
    let (imap, _, paul) = linked_paul(&authority, folders(None)).await;
    let id = format!("i{VALIDITY}.8");
    let result = call(&paul, "mail_read", json!({ "id": id })).await;
    assert!(!result.is_error, "{:?}", result.content);
    let read = &result.content;
    assert_eq!(read["objet"], "Réunion de chantier");
    assert_eq!(read["de"][0]["nom"], "Renée Dupont");
    assert_eq!(read["cc"][0]["adresse"], "lea@exemple.fr");
    assert!(
        read["texte"]
            .as_str()
            .unwrap()
            .contains("réunion de chantier est lundi")
    );
    assert_eq!(read["piecesJointes"][0]["nom"], "devis.pdf");
    assert_eq!(read["lu"], false);
    assert!(read.get("incomplet").is_none());
    assert_eq!(result.citations[0].id, id);
    let fetch = imap
        .heard
        .lines()
        .into_iter()
        .find(|l| l.starts_with("UID FETCH"))
        .unwrap();
    assert_eq!(
        fetch,
        "UID FETCH 8 (UID FLAGS RFC822.SIZE BODY.PEEK[]<0.1048576>)"
    );
}

#[tokio::test]
async fn a_number_given_again_is_never_another_e_mail() {
    let authority = TestAuthority::new();
    let (_, _, paul) = linked_paul(&authority, folders(None)).await;
    // Another validity: the folder was rebuilt since the search.
    let result = call(&paul, "mail_read", json!({ "id": "i1.8" })).await;
    assert!(error_of(&result).starts_with("Cet e-mail n’est plus à cette place"));
    let result = call(
        &paul,
        "mail_read",
        json!({ "id": format!("i{VALIDITY}.9") }),
    )
    .await;
    assert!(error_of(&result).starts_with("Cet e-mail n’est plus à cette place"));
}

#[tokio::test]
async fn sent_e_mails_are_found_where_the_server_keeps_them() {
    let authority = TestAuthority::new();
    for (name, marked) in [("Envoyés perso", true), ("Sent Items", false)] {
        let (imap, _, paul) = linked_paul(&authority, folders(Some((name, marked)))).await;
        let result = call(&paul, "mail_search", json!({ "folder": "sent" })).await;
        assert!(!result.is_error, "{name}: {:?}", result.content);
        assert_eq!(result.content["dossier"], "sent");
        assert_eq!(result.content["emails"][0]["id"], "s42.5");
        assert!(imap.heard.lines().contains(&format!("EXAMINE \"{name}\"")));
        let read = call(&paul, "mail_read", json!({ "id": "s42.5" })).await;
        assert_eq!(read.content["objet"], "Devis envoyé");
    }
    let (_, _, paul) = linked_paul(&authority, folders(Some(("Archives", false)))).await;
    let result = call(&paul, "mail_search", json!({ "folder": "sent" })).await;
    assert!(error_of(&result).starts_with("Le serveur n’indique pas de dossier"));
}

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

/// Paul's tools started on `declared` under `network`, his account linked
/// at `imap`'s server.
async fn started_on(
    declared: &serde_json::Value,
    network: NetworkPolicy,
    ports: &std::sync::Arc<mimorya_agent::testing::InMemoryPorts>,
) -> mimorya_agent::tools::ToolExecutor {
    let tools = tools(declared, network).await;
    let accounts: std::sync::Arc<dyn LinkedAccounts> = ports.clone();
    mailbox_executor(tools, user::PAUL, ports, accounts)
}

#[tokio::test]
async fn the_servers_are_those_the_account_was_checked_against() {
    let authority = TestAuthority::new();
    let (imap, ports) = server(&authority, folders(None), PASSWORD).await;
    // The instance was started on another declaration: the one read with
    // the account is reached.
    let started = declaration("127.0.0.1:1".parse().unwrap());
    let paul = started_on(&started, trusting(&authority), &ports).await;
    let result = call(&paul, "mail_search", json!({})).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert!(!imap.heard.lines().is_empty());
}

#[tokio::test]
async fn a_server_the_policy_no_longer_allows_is_never_reached() {
    let authority = TestAuthority::new();
    let (imap, ports) = server(&authority, folders(None), PASSWORD).await;
    let public = json!({
        "imap": { "host": "imap.exemple.fr", "port": 993 },
        "smtp": { "host": "smtp.exemple.fr", "port": 465, "security": "tls" },
    });
    let narrowed = NetworkPolicy {
        allow_private_network: false,
        ..trusting(&authority)
    };
    let paul = started_on(&public, narrowed, &ports).await;
    let result = call(&paul, "mail_search", json!({})).await;
    assert!(
        error_of(&result).starts_with("Serveur IMAP"),
        "{:?}",
        result.content
    );
    assert!(imap.heard.lines().is_empty());
}

#[tokio::test]
async fn a_refused_account_is_reported_once_and_not_tried_again() {
    let authority = TestAuthority::new();
    let (imap, ports) = server(&authority, folders(None), WRONG).await;
    let paul = started_on(&declaration(imap.address), trusting(&authority), &ports).await;
    let refused = call(&paul, "mail_search", json!({})).await;
    let said = error_of(&refused);
    assert!(said.contains("refuse le compte relié"), "{said}");
    hides(&said, WRONG);
    assert_eq!(ports.refused_mail_accounts(), [linked_at()]);
    // Removed: the next call reaches no server.
    let again = call(&paul, "mail_search", json!({})).await;
    assert!(error_of(&again).contains("n’a relié aucun compte"));
    let logins = imap
        .heard
        .lines()
        .iter()
        .filter(|l| l.starts_with("LOGIN"))
        .count();
    assert_eq!(logins, 1);
}

#[tokio::test]
async fn a_server_refusing_for_now_keeps_the_account() {
    let authority = TestAuthority::new();
    let setup = ImapSetup {
        refuses_for_now: true,
        ..ImapSetup::new(ImapBehaviour::Answers)
    };
    let imap = FakeImap::holding(&authority, accounts(), setup, folders(None)).await;
    let ports = mimorya_agent::testing::InMemoryPorts::new(user::PAUL, uuid::Uuid::new_v4());
    ports.link_mail_account(declaration(imap.address), USER, PASSWORD, linked_at());
    let paul = started_on(&declaration(imap.address), trusting(&authority), &ports).await;
    let refused = call(&paul, "mail_search", json!({})).await;
    assert!(
        error_of(&refused).starts_with(TRY_LATER),
        "{:?}",
        refused.content
    );
    // Its directory down for a while: the account stays, tried again later.
    assert!(ports.refused_mail_accounts().is_empty());
    let again = call(&paul, "mail_search", json!({})).await;
    assert!(error_of(&again).starts_with(TRY_LATER));
}

#[tokio::test]
async fn without_an_account_nothing_is_reached() {
    let authority = TestAuthority::new();
    let (imap, _) = server(&authority, folders(None), PASSWORD).await;
    let ports = mimorya_agent::testing::InMemoryPorts::new(user::PAUL, uuid::Uuid::new_v4());
    let paul = started_on(&declaration(imap.address), MAIL_LOCAL, &ports).await;
    let result = call(
        &paul,
        "mail_read",
        json!({ "id": format!("i{VALIDITY}.8") }),
    )
    .await;
    assert!(error_of(&result).contains("dans « Mon compte »"));
    assert!(imap.heard.lines().is_empty());
    // A malformed name is refused before anything.
    let evaluation = paul.evaluate(&mimorya_agent::llm::LlmToolCall {
        id: "c".into(),
        name: "mail_read".into(),
        input: json!({ "id": "i1.2\r\nA9 LOGOUT" }),
    });
    assert!(matches!(
        evaluation,
        mimorya_agent::tools::ToolCallEvaluation::Deny { .. }
    ));
}
