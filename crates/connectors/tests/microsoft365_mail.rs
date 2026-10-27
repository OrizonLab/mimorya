//! The Microsoft 365 e-mail tools (ADR-117): what they ask Graph with the
//! person's token, what they keep of its answer, and what they say of its
//! refusals, without Graph's own words.

mod support;

use std::sync::Arc;

use mimorya_agent::testing::InMemoryPorts;
use mimorya_connectors::microsoft365::{MICROSOFT365_TOOLS, MICROSOFT365_WRITE_TOOLS};
use mimorya_connectors::testing::{FORBIDDEN_MESSAGE, FakeGraph, GRAPH_DETAIL, OVERSIZED_MESSAGE};
use mimorya_core::account_links::MICROSOFT365;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::demo::user;
use mimorya_core::tools::LinkedAccounts;
use serde_json::json;
use support::microsoft365::{call, error_of, mailbox_executor, microsoft365_tools};
use uuid::Uuid;

const TOKEN: &str = "acces-lea-1";

async fn lea(graph: &FakeGraph) -> mimorya_agent::tools::ToolExecutor {
    let ports = InMemoryPorts::new(user::LEA, Uuid::new_v4());
    ports.link_account(MICROSOFT365, TOKEN);
    let accounts: Arc<dyn LinkedAccounts> = ports.clone();
    mailbox_executor(
        microsoft365_tools(&graph.url).await,
        user::LEA,
        &ports,
        accounts,
    )
}

#[tokio::test]
async fn its_tools_read_with_the_connectors_permission() {
    let graph = FakeGraph::start().await;
    let tools = microsoft365_tools(&graph.url).await;
    let names: Vec<String> = tools.iter().map(|t| t.info().name.clone()).collect();
    // Those that write wait for the company to let them (ADR-119).
    let reading: Vec<&str> = MICROSOFT365_TOOLS
        .into_iter()
        .filter(|name| !MICROSOFT365_WRITE_TOOLS.contains(name))
        .collect();
    assert_eq!(names, reading);
    for tool in &tools {
        let info = tool.info();
        assert_eq!(info.risk, ToolRisk::Read, "{}", info.name);
        assert_eq!(info.required_permissions, ["connector:use"]);
        assert!(!info.requires_approval);
    }
}

#[tokio::test]
async fn words_are_searched_in_the_persons_mailbox() {
    let graph = FakeGraph::start().await;
    let executor = lea(&graph).await;
    let result = call(
        &executor,
        "m365_mail_search",
        json!({ "query": "facture \"mars\"" }),
    )
    .await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["nombre"], 1);
    let found = &result.content["emails"][0];
    assert_eq!(found["objet"], "Facture de mars — lea");
    assert_eq!(found["de"], "Fournisseur <contact@fournisseur.example>");
    assert_eq!(found["piecesJointes"], true);
    assert_eq!(result.citations.len(), 1);
    assert_eq!(result.citations[0].kind, "email");
    assert_eq!(result.citations[0].id, "lea/m2=");
    let asked = &graph.requests()[0];
    assert_eq!(asked.token, TOKEN);
    assert_eq!(asked.path, "/me/messages");
    // The words within quotes, without their own.
    assert_eq!(asked.query["$search"], "\"facture mars\"");
    assert_eq!(asked.query["$top"], "10");
    assert!(!asked.query.contains_key("$orderby"));
}

#[tokio::test]
async fn the_latest_are_read_since_a_day_of_the_platforms_zone() {
    let graph = FakeGraph::start().await;
    let executor = lea(&graph).await;
    let result = call(
        &executor,
        "m365_mail_search",
        json!({ "since": "2026-10-01", "limit": 2 }),
    )
    .await;
    assert_eq!(result.content["nombre"], 2);
    assert!(
        result.content["autres"]
            .as_str()
            .unwrap()
            .contains("précisez")
    );
    let asked = &graph.requests()[0];
    assert_eq!(asked.query["$orderby"], "receivedDateTime desc");
    assert_eq!(
        asked.query["$filter"],
        "receivedDateTime ge 2026-09-30T22:00:00Z"
    );
    assert_eq!(asked.query["$top"], "2");
}

#[tokio::test]
async fn words_and_a_day_are_refused_together_before_graph() {
    let graph = FakeGraph::start().await;
    let executor = lea(&graph).await;
    let both = json!({ "query": "facture", "since": "2026-10-01" });
    let error = error_of(&call(&executor, "m365_mail_search", both).await);
    assert!(error.contains("pas les deux"), "{error}");
    let too_many = executor.evaluate(&mimorya_agent::llm::LlmToolCall {
        id: "c".into(),
        name: "m365_mail_search".into(),
        input: json!({ "limit": 26 }),
    });
    assert!(matches!(
        too_many,
        mimorya_agent::tools::ToolCallEvaluation::Deny { .. }
    ));
    assert!(graph.requests().is_empty());
}

#[tokio::test]
async fn an_email_is_read_as_text_its_identifier_encoded_whole() {
    let graph = FakeGraph::start().await;
    let executor = lea(&graph).await;
    let result = call(&executor, "m365_mail_read", json!({ "id": "lea/m2=" })).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(result.content["texte"], "Bonjour lea,\n\nFacture de mars.");
    assert_eq!(result.content["texteTronque"], false);
    assert_eq!(result.content["a"], json!(["lea <lea@exemple.fr>"]));
    assert_eq!(result.citations[0].id, "lea/m2=");
    let asked = &graph.requests()[0];
    assert_eq!(asked.path, "/me/messages/lea/m2=");
    assert_eq!(
        asked.prefer.as_deref(),
        Some("outlook.body-content-type=\"text\"")
    );
}

#[tokio::test]
async fn graphs_refusals_are_told_without_its_words() {
    let graph = FakeGraph::start().await;
    let executor = lea(&graph).await;
    let forbidden = error_of(
        &call(
            &executor,
            "m365_mail_read",
            json!({ "id": FORBIDDEN_MESSAGE }),
        )
        .await,
    );
    assert!(forbidden.contains("permission"), "{forbidden}");
    // Another person's e-mail is not in this mailbox.
    let theirs = error_of(&call(&executor, "m365_mail_read", json!({ "id": "marc/m2=" })).await);
    assert!(theirs.contains("Introuvable"), "{theirs}");
    let huge = error_of(
        &call(
            &executor,
            "m365_mail_read",
            json!({ "id": OVERSIZED_MESSAGE }),
        )
        .await,
    );
    assert!(huge.contains("trop longue"), "{huge}");
    for told in [forbidden, theirs, huge] {
        assert!(!told.contains(GRAPH_DETAIL), "{told}");
    }
    let refused = executor.evaluate(&mimorya_agent::llm::LlmToolCall {
        id: "c".into(),
        name: "m365_mail_read".into(),
        input: json!({ "id": "../../users/marc/messages" }),
    });
    assert!(matches!(
        refused,
        mimorya_agent::tools::ToolCallEvaluation::Deny { .. }
    ));
}
