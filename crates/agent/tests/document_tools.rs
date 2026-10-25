//! The document tools (ADR-096), on the demo company: a run finds and reads
//! the documents its user may read only, through the agent, each found
//! document cited once; one out of reach answers as absent. Their inputs
//! are strict and bounded.

use std::sync::Arc;

use mimorya_agent::testing::{DocumentFixture, InMemoryPorts};
use mimorya_agent::tools::platform::platform_tools;
use mimorya_core::authz::{Actor, Sensitivity};
use mimorya_core::demo::{self, unit, user};
use mimorya_core::documents::DocumentSpace;
use mimorya_core::tools::{BoundAuthorization, ToolContext, ToolRef, ToolResult};
use serde_json::{Value, json};
use uuid::Uuid;

struct Harness {
    tools: Vec<ToolRef>,
    ctx: ToolContext,
}

impl Harness {
    /// The platform tools of a run of `user_id`, `documents` deposited.
    fn new(user_id: Uuid, documents: &[DocumentFixture]) -> Self {
        let run_id = Uuid::new_v4();
        let ports = InMemoryPorts::new(user_id, run_id);
        for document in documents {
            ports.deposit(document.clone());
        }
        let engine = Arc::new(demo::authorization_engine());
        let subject = Arc::new(demo::subject(user_id));
        let actor = Actor::agent(Some(run_id));
        let engine_ports = ports.ports();
        let ctx = ToolContext {
            tenant_id: subject.tenant_id,
            subject: subject.clone(),
            actor: actor.clone(),
            run_id,
            authz: BoundAuthorization::new(engine, subject, actor),
            deliverables: engine_ports.deliverables.clone(),
        };
        let tools = platform_tools(ports.clone(), ports.clone(), ports);
        Self { tools, ctx }
    }

    fn tool(&self, name: &str) -> &ToolRef {
        self.tools.iter().find(|t| t.info().name == name).unwrap()
    }

    async fn call(&self, name: &str, input: Value) -> ToolResult {
        let tool = self.tool(name);
        let input = tool.parse_input(&input).unwrap();
        tool.execute(input, &self.ctx).await.unwrap()
    }
}

fn in_space(title: &str, space: DocumentSpace, text: &str) -> DocumentFixture {
    DocumentFixture {
        space,
        ..DocumentFixture::new(title, &[text])
    }
}

#[tokio::test]
async fn a_run_finds_and_reads_what_its_user_may_read_only() {
    let company = DocumentFixture::new(
        "Charte",
        &["La purge du chauffe-eau est notée.", "Second passage."],
    );
    let north = in_space(
        "Nord",
        DocumentSpace::Unit(unit::TEAM_NORTH),
        "Purge du chauffe-eau le lundi.",
    );
    let south = in_space(
        "Sud",
        DocumentSpace::Unit(unit::TEAM_SOUTH),
        "Purge du chauffe-eau le mardi.",
    );
    let priced = DocumentFixture {
        sensitivity: Sensitivity::Confidential,
        ..in_space(
            "Tarifs",
            DocumentSpace::Unit(unit::TECHNICAL),
            "Purge du chauffe-eau : 90 euros.",
        )
    };
    let all = [
        company.clone(),
        north.clone(),
        south.clone(),
        priced.clone(),
    ];
    let lucas = Harness::new(user::LUCAS, &all);

    let found = lucas
        .call("document_search", json!({ "query": "purge chauffe-eau" }))
        .await;
    let documents: Vec<&Value> = found
        .content
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| &hit["document"])
        .collect();
    assert_eq!(documents.len(), 2, "{}", found.content);
    for id in [company.id, north.id] {
        assert!(documents.contains(&&json!(id)), "{}", found.content);
    }
    let cited: Vec<&str> = found.citations.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(cited.len(), 2);

    let read = lucas
        .call("document_read", json!({ "document": company.id }))
        .await;
    assert!(!read.is_error);
    assert_eq!(read.content["total"], 2);
    assert_eq!(read.content["suite"], Value::Null);
    assert_eq!(read.content["passages"][1]["texte"], "Second passage.");
    for absent in [south.id, priced.id, Uuid::new_v4()] {
        let refused = lucas
            .call("document_read", json!({ "document": absent }))
            .await;
        assert!(refused.is_error);
        assert_eq!(
            refused.content,
            json!({ "erreur": "Document introuvable ou non accessible" })
        );
        assert!(refused.citations.is_empty());
    }

    // Marc, over the department, cleared to confidential: all four.
    let marc = Harness::new(user::MARC, &all);
    let found = marc
        .call("document_search", json!({ "query": "purge chauffe-eau" }))
        .await;
    assert_eq!(found.content.as_array().unwrap().len(), 4);
}

#[test]
fn their_inputs_are_strict_and_bounded() {
    let h = Harness::new(user::LUCAS, &[]);
    let search = h.tool("document_search");
    let read = h.tool("document_read");
    for refused in [
        json!({ "query": "x" }),
        json!({ "query": "purge", "limit": 11 }),
        json!({ "query": "purge", "limit": 0 }),
        json!({ "query": "purge", "scope": "all" }),
    ] {
        assert!(search.parse_input(&refused).is_err(), "{refused}");
    }
    for refused in [
        json!({ "document": "pas-un-identifiant" }),
        json!({ "document": Uuid::new_v4(), "from": 2000 }),
        json!({ "document": Uuid::new_v4(), "all": true }),
    ] {
        assert!(read.parse_input(&refused).is_err(), "{refused}");
    }
    assert!(
        read.parse_input(&json!({ "document": Uuid::new_v4(), "from": 1999 }))
            .is_ok()
    );
    for tool in [search, read] {
        assert_eq!(tool.info().required_permissions, ["document:read"]);
    }
}
