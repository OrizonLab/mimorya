//! The Gmail tools (ADR-122): what they ask Gmail with the person's token,
//! what they keep of its answers (the headers of each e-mail found, the
//! text of one in its charset, its attachments named), and what they say
//! of its refusals, without Google's own words.

mod support;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_agent::tools::ToolExecutor;
use mimorya_connectors::google_workspace::GOOGLE_WORKSPACE_TOOLS;
use mimorya_connectors::testing::{
    FORBIDDEN_MESSAGE, FakeGoogle, GOOGLE_DETAIL, LISTED_GONE, LISTED_REFUSED, OVERSIZED_MESSAGE,
};
use mimorya_core::account_links::{GMAIL_READONLY, GOOGLE_WORKSPACE};
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use serde_json::json;
use support::google_workspace::google_tools;
use support::linked_accounts::{call, error_of, mailbox_executor};
use uuid::Uuid;

const TOKEN: &str = "acces-lea-1";

async fn lea(google: &FakeGoogle) -> ToolExecutor {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account(GOOGLE_WORKSPACE, TOKEN);
    mailbox_executor(google_tools(google).await, user::LEA, &ports, ports.clone())
}

#[tokio::test]
async fn its_tools_read_with_the_connectors_permission() {
    let google = FakeGoogle::start().await;
    let tools = google_tools(&google).await;
    let names: Vec<String> = tools.iter().map(|t| t.info().name.clone()).collect();
    assert_eq!(names, GOOGLE_WORKSPACE_TOOLS);
    for tool in &tools {
        let info = tool.info();
        assert_eq!(info.risk, ToolRisk::Read, "{}", info.name);
        assert_eq!(info.required_permissions, ["connector:use"]);
        assert!(!info.requires_approval);
    }
}

#[tokio::test]
async fn gmail_searches_the_persons_mailbox_and_reads_each_ones_headers() {
    let google = FakeGoogle::start().await;
    let executor = lea(&google).await;
    let result = call(
        &executor,
        "google_gmail_search",
        json!({ "query": "facture\u{0} from:fournisseur", "since": "2026-10-01" }),
    )
    .await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 1);
    let found = &result.content["emails"][0];
    assert_eq!(found["id"], "m2");
    assert_eq!(found["objet"], "Facture de mars — lea");
    assert_eq!(found["de"], "Fournisseur <contact@fournisseur.example>");
    assert_eq!(found["recu"], "2025-10-20T09:00:00Z");
    assert_eq!(found["lu"], false);
    assert_eq!(found["lien"], "https://mail.google.com/mail/#all/m2");
    assert_eq!(result.citations[0].id, "m2");
    assert_eq!(
        result.citations[0].label.as_deref(),
        Some("Facture de mars — lea")
    );
    let asked = google.requests();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(asked.iter().all(|request| request.token == TOKEN));
    assert_eq!(asked[0].path, "/users/me/messages");
    // The words on one line, the day as the instant it starts in Paris.
    assert_eq!(
        asked[0].param("q"),
        Some("facture from:fournisseur after:1790805600")
    );
    assert_eq!(asked[0].param("maxResults"), Some("10"));
    // The headers of the one found only.
    assert_eq!(asked[1].path, "/users/me/messages/m2");
    assert_eq!(asked[1].param("format"), Some("metadata"));
    let headers: Vec<&str> = asked[1]
        .query
        .iter()
        .filter(|(name, _)| name == "metadataHeaders")
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(headers, ["Subject", "From"]);
}

#[tokio::test]
async fn the_latest_are_listed_and_more_are_said() {
    let google = FakeGoogle::start().await;
    let executor = lea(&google).await;
    let result = call(&executor, "google_gmail_search", json!({ "limit": 2 })).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 2);
    assert_eq!(result.content["emails"][0]["id"], "m3");
    // The snippet's entities decoded.
    assert_eq!(
        result.content["emails"][0]["apercu"],
        "Planning de novembre & congés — lea"
    );
    assert!(
        result.content["autres"]
            .as_str()
            .unwrap()
            .contains("précisez")
    );
    assert_eq!(google.requests()[0].param("q"), None);
}

#[tokio::test]
async fn an_email_is_read_in_its_charset_with_its_attachments_named() {
    let google = FakeGoogle::start().await;
    let executor = lea(&google).await;
    let result = call(&executor, "google_gmail_read", json!({ "id": "m2" })).await;
    assert!(!result.is_error, "{:?}", result.content);
    let read = &result.content;
    // The plain text part, in ISO-8859-1, rather than the HTML one.
    assert_eq!(read["texte"], "Préparé pour lea.");
    assert_eq!(read["texteTronque"], false);
    assert_eq!(read["a"], "lea <lea@exemple.fr>");
    assert_eq!(read["cc"], "Compta <compta@exemple.fr>");
    assert_eq!(read["piecesJointes"], json!(["facture-mars.pdf"]));
    assert_eq!(result.citations[0].id, "m2");
    let asked = &google.requests()[0];
    assert_eq!(asked.path, "/users/me/messages/m2");
    assert_eq!(asked.param("format"), Some("full"));
    // An e-mail in HTML only: its text.
    let html = call(&executor, "google_gmail_read", json!({ "id": "m3" })).await;
    assert_eq!(html.content["texte"], "Bonjour lea,\nLe planning.");
}

#[tokio::test]
async fn refusals_are_said_without_googles_words() {
    let google = FakeGoogle::start().await;
    let executor = lea(&google).await;
    let forbidden = error_of(
        &call(
            &executor,
            "google_gmail_read",
            json!({ "id": FORBIDDEN_MESSAGE }),
        )
        .await,
    );
    assert!(
        forbidden.starts_with("Google Workspace refuse"),
        "{forbidden}"
    );
    let oversized = error_of(
        &call(
            &executor,
            "google_gmail_read",
            json!({ "id": OVERSIZED_MESSAGE }),
        )
        .await,
    );
    assert!(oversized.contains("trop longue"), "{oversized}");
    // Another person's e-mail is not in her mailbox.
    let missing = error_of(&call(&executor, "google_gmail_read", json!({ "id": "m9" })).await);
    assert_eq!(missing, "Introuvable dans votre compte Google Workspace");
    // An identifier that is not Gmail's never reaches it.
    let path = json!({ "id": "m2/../../settings" });
    let refused = executor.evaluate(&mimorya_agent::llm::LlmToolCall {
        id: "c".into(),
        name: "google_gmail_read".into(),
        input: path,
    });
    assert!(matches!(
        refused,
        mimorya_agent::tools::ToolCallEvaluation::Deny { .. }
    ));
    for told in [&forbidden, &oversized, &missing] {
        assert!(!told.contains(GOOGLE_DETAIL), "{told}");
    }
    assert_eq!(google.requests().len(), 3);
}

#[tokio::test]
async fn a_refused_token_is_reported_and_not_granted_reading_never_reaches_google() {
    let google = FakeGoogle::start().await;
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account(GOOGLE_WORKSPACE, TOKEN);
    let executor = mailbox_executor(
        google_tools(&google).await,
        user::LEA,
        &ports,
        ports.clone(),
    );
    google.refuse(TOKEN);
    let refused = error_of(&call(&executor, "google_gmail_search", json!({})).await);
    assert!(refused.contains("relie de nouveau"), "{refused}");
    assert_eq!(ports.rejected_accounts(), [GOOGLE_WORKSPACE]);

    // Linked without reading Gmail: refused before Google.
    let calendar_only = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    calendar_only.link_account_granting(
        GOOGLE_WORKSPACE,
        "acces-lea-2",
        "openid https://www.googleapis.com/auth/calendar.readonly",
    );
    let executor = mailbox_executor(
        google_tools(&google).await,
        user::LEA,
        &calendar_only,
        calendar_only.clone(),
    );
    let before = google.requests().len();
    let told = error_of(&call(&executor, "google_gmail_search", json!({})).await);
    assert!(told.contains("la lecture de ses e-mails"), "{told}");
    assert!(told.contains("Google Workspace"), "{told}");
    assert_eq!(google.requests().len(), before);
    // The scope as Google names it.
    assert!(GMAIL_READONLY.ends_with("gmail.readonly"));
}

#[tokio::test]
async fn an_email_gone_since_it_was_listed_is_passed_over() {
    let google = FakeGoogle::start().await;
    let executor = lea(&google).await;
    let result = call(
        &executor,
        "google_gmail_search",
        json!({ "query": LISTED_GONE }),
    )
    .await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 1);
    assert_eq!(result.content["emails"][0]["id"], "m2");
}

#[tokio::test]
async fn the_first_refusal_stops_the_reads() {
    let google = FakeGoogle::start().await;
    let executor = lea(&google).await;
    let result = call(
        &executor,
        "google_gmail_search",
        json!({ "query": LISTED_REFUSED, "limit": 25 }),
    )
    .await;
    let told = error_of(&result);
    assert!(told.starts_with("Google Workspace refuse"), "{told}");
    assert!(!told.contains(GOOGLE_DETAIL));
    // The list, then the reads already under way: not the ten.
    let asked = google.requests().len();
    assert!(asked <= 6, "{asked} requests");
}
