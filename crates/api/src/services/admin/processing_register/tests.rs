use chrono::Utc;
use mimorya_agent::llm::PROVIDER_KINDS;
use mimorya_agent::llm::embedding::EMBEDDING_KINDS;
use mimorya_connectors::connector_types;
use mimorya_core::processing_register::{
    ActivityKind, ChosenBy, ConnectorKind, DecisionUses, KeptData, ModelService, Place,
    RegisterFacts, register,
};
use mimorya_core::second_factor::Requirement;
use serde_json::{Value, json};
use uuid::Uuid;

use super::reading::{connector, decision_uses, host, listed_host, platform_model};

fn declared(
    connector_type: &str,
    config: Value,
) -> mimorya_core::processing_register::ConnectorFact {
    connector(
        &mimorya_db::repositories::connectors::ConnectorInstanceRow {
            id: Uuid::nil(),
            tenant_id: Uuid::nil(),
            connector_type: connector_type.into(),
            name: "Déclaré".into(),
            enabled: true,
            config,
            tool_policies: json!({}),
            status: "ok".into(),
            secret_id: None,
            last_error: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        KeptData::default(),
    )
}

/// Where the register places the model of the company's runs `service`.
fn placed(service: ModelService) -> (String, Place) {
    let facts = RegisterFacts {
        company: "Néo".into(),
        runs_model: Some(service),
        review_model: None,
        embeddings: None,
        decisions: DecisionUses::default(),
        jev: false,
        connectors: Vec::new(),
        gateways: Vec::new(),
        identity_providers: Vec::new(),
        scim: false,
        audit_export: false,
        mail_relay: None,
        retention: Vec::new(),
        second_factor: Requirement::Optional,
    };
    let register = register(&facts);
    let assistance = register
        .activities
        .iter()
        .find(|activity| activity.kind == ActivityKind::Assistance)
        .unwrap();
    let recipient = &assistance.recipients[0];
    (recipient.name.clone(), recipient.place)
}

#[test]
fn the_decision_service_is_asked_the_usages_set_and_delivered_only() {
    let uses = decision_uses(
        [
            ("model_routing", "observe"),
            ("memory_guard", "off"),
            ("document_labels", "active"),
            ("mail_triage", "active"),
            ("inconnu", "active"),
            ("action_guard", "inconnu"),
        ]
        .into_iter(),
    );
    assert_eq!(
        uses,
        DecisionUses {
            requests: true,
            documents: true,
            ..DecisionUses::default()
        }
    );
}

#[test]
fn a_listed_endpoint_is_named_by_its_host_alone() {
    assert_eq!(
        listed_host(
            true,
            Some("http://jean:secret@Modeles.Interne:11434/v1?k=1")
        ),
        Some("modeles.interne".into())
    );
    assert_eq!(listed_host(false, Some("https://api.example.com")), None);
    assert_eq!(listed_host(true, None), None);
    assert_eq!(host("urn:example:idp"), None);
}

#[test]
fn the_operator_s_model_is_named_where_it_is_reached() {
    // At its provider's own address, set or not.
    for (kind, base_url) in [
        ("anthropic", None),
        ("anthropic", Some("https://api.anthropic.com/")),
        ("openai", None),
        ("gemini", None),
    ] {
        let model = platform_model(kind, base_url);
        assert_eq!(model.server_host, None, "{kind} {base_url:?}");
        assert_eq!(model.chosen_by, ChosenBy::Operator);
        assert_ne!(placed(model).1, Place::Unknown);
    }
    // Elsewhere: by the host the operator set, or the kind's own.
    let elsewhere = platform_model("openai", Some("https://passerelle.exploitant.example/v1"));
    assert_eq!(
        placed(elsewhere),
        (
            "Serveur de modèles désigné par l’exploitant (passerelle.exploitant.example), API OpenAI"
                .into(),
            Place::OperatorServer
        )
    );
    let local = platform_model("ollama", None);
    assert_eq!(local.server_host.as_deref(), Some("localhost"));
}

#[test]
fn every_kind_of_model_has_a_known_place() {
    for kind in PROVIDER_KINDS.iter().chain(EMBEDDING_KINDS.iter()) {
        let listed = mimorya_agent::llm::uses_listed_endpoint(kind)
            || mimorya_agent::llm::embedding::embedding_uses_listed_endpoint(kind);
        let service = ModelService {
            provider: kind.to_string(),
            server_host: listed.then(|| "modeles.interne".to_string()),
            chosen_by: ChosenBy::Company,
        };
        assert_ne!(placed(service).1, Place::Unknown, "{kind}");
    }
}

#[test]
fn every_connector_type_is_read_by_what_it_reaches() {
    for definition in connector_types() {
        let kind = declared(definition.info().connector_type, json!({})).kind;
        assert_ne!(
            kind,
            ConnectorKind::Other,
            "{}",
            definition.info().connector_type
        );
    }
    assert_eq!(declared("inconnu", json!({})).kind, ConnectorKind::Other);
}

#[test]
fn a_connector_is_read_as_its_declaration_says() {
    let http = declared(
        "mcp",
        json!({ "transport": "http", "url": "https://MCP.example/mcp" }),
    );
    assert_eq!(
        (http.kind, http.hosts),
        (ConnectorKind::Mcp, vec!["mcp.example".into()])
    );
    let process = declared(
        "mcp",
        json!({ "transport": "stdio", "command": "mcp-demo-docs" }),
    );
    assert_eq!(
        (process.kind, process.hosts),
        (ConnectorKind::McpProcess, Vec::new())
    );
    let mail = declared(
        mimorya_connectors::standard_mail::MAIL_SERVER,
        json!({
            "imap": { "host": "imap.example.com", "port": 993 },
            "smtp": { "host": "smtp.example.com", "port": 465, "security": "tls" },
        }),
    );
    assert_eq!(mail.kind, ConnectorKind::StandardMail);
    assert_eq!(mail.hosts, ["imap.example.com", "smtp.example.com"]);
    let unreadable = declared(
        mimorya_connectors::standard_mail::MAIL_SERVER,
        json!({ "imap": "?" }),
    );
    assert!(unreadable.hosts.is_empty());
}
