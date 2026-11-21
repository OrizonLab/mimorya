use chrono::{TimeZone, Utc};

use super::*;
use crate::identity_providers::ProviderKind;
use crate::retention::RetentionKind;
use crate::second_factor::Requirement;

fn bare() -> RegisterFacts {
    RegisterFacts {
        company: "Néo Maintenance".into(),
        runs_model: None,
        review_model: None,
        embeddings: None,
        jev: JevUses::default(),
        connectors: Vec::new(),
        gateways: Vec::new(),
        identity_providers: Vec::new(),
        scim: false,
        audit_export: false,
        mails: false,
        retention: Vec::new(),
        second_factor: Requirement::Optional,
    }
}

fn own(provider: &str) -> ModelService {
    ModelService {
        provider: provider.into(),
        listed_host: None,
        chosen_by: ChosenBy::Company,
    }
}

fn activity(register: &Register, kind: ActivityKind) -> &Activity {
    register
        .activities
        .iter()
        .find(|activity| activity.kind == kind)
        .expect("the activity")
}

fn names(activity: &Activity) -> Vec<&str> {
    activity
        .recipients
        .iter()
        .map(|r| r.name.as_str())
        .collect()
}

#[test]
fn a_company_without_services_has_its_activities_and_no_recipient() {
    let register = register(&bare());
    assert_eq!(register.controller, "Néo Maintenance");
    let kinds: Vec<&str> = register.activities.iter().map(|a| a.kind.key()).collect();
    assert_eq!(
        kinds,
        [
            "accounts",
            "assistance",
            "memory",
            "documents",
            "audit",
            "usage"
        ]
    );
    assert!(register.activities.iter().all(|a| a.recipients.is_empty()));
    // The platform's durations, without the company's.
    let assistance = activity(&register, ActivityKind::Assistance);
    assert!(assistance.retention[0].contains("depuis 24 mois"));
    let memory = activity(&register, ActivityKind::Memory);
    assert!(memory.retention[0].contains("12 mois"));
    assert!(memory.retention[1].contains("30 jours"));
    assert_eq!(register.to_complete.len(), 6);
}

#[test]
fn models_are_named_with_where_they_stand_and_who_chose_them() {
    let mut facts = bare();
    facts.runs_model = Some(own("anthropic"));
    facts.review_model = Some(ModelService {
        provider: "ollama".into(),
        listed_host: Some("modeles.interne".into()),
        chosen_by: ChosenBy::Company,
    });
    facts.embeddings = Some(own("mistral"));
    let register = register(&facts);
    let assistance = activity(&register, ActivityKind::Assistance);
    assert_eq!(
        names(assistance),
        [
            "Anthropic",
            "Serveur de modèles listé par l’exploitant (modeles.interne)"
        ]
    );
    assert_eq!(assistance.recipients[0].place, Place::OutsideEuropeanUnion);
    assert_eq!(assistance.recipients[0].chosen_by, ChosenBy::Company);
    assert_eq!(assistance.recipients[1].place, Place::ListedByOperator);
    let memory = activity(&register, ActivityKind::Memory);
    assert_eq!(names(memory), ["Anthropic", "Mistral AI"]);
    assert_eq!(memory.recipients[1].place, Place::EuropeanUnion);
    let documents = activity(&register, ActivityKind::Documents);
    assert_eq!(names(documents), ["Anthropic", "Mistral AI"]);
    // What its connectors read goes to the model of its runs.
    facts.connectors = vec![ConnectorFact {
        kind: ConnectorKind::Microsoft365,
        name: "Microsoft 365".into(),
        hosts: Vec::new(),
    }];
    let register = super::register(&facts);
    let connectors = activity(&register, ActivityKind::Connectors);
    assert_eq!(
        names(connectors),
        ["Anthropic", "Microsoft 365 (Microsoft Graph)"]
    );
}

#[test]
fn the_operator_s_model_is_its_processor_and_an_unknown_kind_is_said_so() {
    let mut facts = bare();
    facts.runs_model = Some(ModelService {
        provider: "anthropic".into(),
        listed_host: None,
        chosen_by: ChosenBy::Operator,
    });
    facts.embeddings = Some(own("inconnu"));
    let register = register(&facts);
    let recipient = &activity(&register, ActivityKind::Assistance).recipients[0];
    assert_eq!(recipient.chosen_by, ChosenBy::Operator);
    assert!(recipient.chosen_by.label().contains("exploitant"));
    let memory = activity(&register, ActivityKind::Memory);
    assert_eq!(memory.recipients[1].name, "Fournisseur « inconnu »");
    assert_eq!(memory.recipients[1].place, Place::Unknown);
}

#[test]
fn jev_receives_only_what_the_usages_set_send_it() {
    let mut facts = bare();
    facts.jev = JevUses {
        memories: true,
        external_contents: true,
        ..JevUses::default()
    };
    facts.connectors = vec![ConnectorFact {
        kind: ConnectorKind::Rest,
        name: "Facturation".into(),
        hosts: vec!["api.facturation.example".into()],
    }];
    let register = register(&facts);
    let typesafe = |kind| {
        activity(&register, kind)
            .recipients
            .iter()
            .filter(|r| r.name == "TypeSafe (Jev)")
            .count()
    };
    assert_eq!(typesafe(ActivityKind::Assistance), 0);
    assert_eq!(typesafe(ActivityKind::Memory), 1);
    assert_eq!(typesafe(ActivityKind::Documents), 0);
    assert_eq!(typesafe(ActivityKind::Connectors), 1);
    let jev = activity(&register, ActivityKind::Memory)
        .recipients
        .iter()
        .find(|r| r.name == "TypeSafe (Jev)")
        .unwrap();
    assert_eq!(jev.place, Place::OutsideEuropeanUnion);
    assert!(
        JevUses {
            requests: true,
            ..JevUses::default()
        }
        .any()
    );
    assert!(!JevUses::default().any());
}

#[test]
fn each_connector_is_named_by_what_its_tools_reach() {
    let connector = |kind, name: &str, hosts: &[&str]| ConnectorFact {
        kind,
        name: name.into(),
        hosts: hosts.iter().map(|h| h.to_string()).collect(),
    };
    let mut facts = bare();
    facts.connectors = vec![
        connector(ConnectorKind::Demo, "Interventions", &[]),
        connector(ConnectorKind::Mcp, "Docs", &["mcp.example.com"]),
        connector(ConnectorKind::McpProcess, "Docs locaux", &[]),
        connector(ConnectorKind::Mcp, "Docs illisibles", &[]),
        connector(
            ConnectorKind::StandardMail,
            "Messagerie",
            &["imap.example.com", "smtp.example.com"],
        ),
        connector(ConnectorKind::Microsoft365, "Microsoft 365", &[]),
        connector(ConnectorKind::GoogleWorkspace, "Google", &[]),
    ];
    let register = register(&facts);
    let connectors = activity(&register, ActivityKind::Connectors);
    assert_eq!(
        names(connectors),
        [
            "Serveur MCP (mcp.example.com)",
            "Serveur MCP lancé par la plateforme",
            "Serveur MCP",
            "Serveurs de messagerie (imap.example.com, smtp.example.com)",
            "Microsoft 365 (Microsoft Graph)",
            "Google Workspace (Gmail, Google Agenda)",
        ]
    );
    assert_eq!(connectors.recipients[0].role, "Connecteur « Docs »");
    assert_eq!(connectors.recipients[0].place, Place::DeclaredByCompany);
    assert_eq!(connectors.recipients[1].place, Place::Operator);
    assert_eq!(connectors.recipients[2].place, Place::DeclaredByCompany);
    assert_eq!(connectors.recipients[4].place, Place::OutsideEuropeanUnion);
    assert_eq!(connectors.notes.len(), 1);
    assert!(connectors.notes[0].contains("« Interventions »"));
    assert!(connectors.notes[0].contains("fictives"));
}

#[test]
fn gateways_and_sign_in_name_their_services() {
    let mut facts = bare();
    facts.gateways = vec![GatewayKind::Slack, GatewayKind::Teams];
    facts.identity_providers = vec![
        IdentityFact {
            kind: ProviderKind::Oidc,
            name: "Keycloak".into(),
            host: Some("sso.example.com".into()),
        },
        IdentityFact {
            kind: ProviderKind::Saml,
            name: "AD FS".into(),
            host: None,
        },
        IdentityFact {
            kind: ProviderKind::Microsoft,
            name: "Microsoft 365".into(),
            host: Some("login.microsoftonline.com".into()),
        },
    ];
    facts.scim = true;
    facts.mails = true;
    facts.audit_export = true;
    let register = register(&facts);
    assert!(
        register
            .activities
            .iter()
            .all(|a| a.kind != ActivityKind::Connectors)
    );
    let gateways = activity(&register, ActivityKind::Gateways);
    assert_eq!(
        names(gateways),
        ["Slack", "Microsoft Teams (Bot Framework)"]
    );
    let accounts = activity(&register, ActivityKind::Accounts);
    assert_eq!(
        names(accounts),
        [
            "Fournisseur OpenID Connect (sso.example.com)",
            "Fournisseur SAML",
            "Microsoft Entra ID",
            "Fournisseur d’identité de l’entreprise",
            "Relais d’envoi des e-mails de l’exploitant",
        ]
    );
    assert_eq!(
        accounts.recipients[0].role,
        "Fournisseur d’identité « Keycloak »"
    );
    assert_eq!(accounts.recipients[4].chosen_by, ChosenBy::Operator);
    let audit = activity(&register, ActivityKind::Audit);
    assert_eq!(names(audit), ["SIEM de l’entreprise"]);
    assert!(
        register
            .measures
            .iter()
            .any(|m| m.contains("fournisseur d’identité"))
    );
}

#[test]
fn durations_are_those_in_effect_a_shortening_said_with_its_day() {
    let mut facts = bare();
    facts.retention = vec![
        RetentionFact {
            kind: RetentionKind::Conversations,
            in_effect: 24,
            chosen: 12,
            shortening_at: Some(Utc.with_ymd_and_hms(2026, 11, 27, 23, 30, 0).unwrap()),
        },
        RetentionFact {
            kind: RetentionKind::Episodes,
            in_effect: 6,
            chosen: 6,
            shortening_at: None,
        },
        RetentionFact {
            kind: RetentionKind::Departed,
            in_effect: 7,
            chosen: 7,
            shortening_at: None,
        },
    ];
    let register = register(&facts);
    let assistance = activity(&register, ActivityKind::Assistance);
    // Paris is an hour ahead of UTC in November.
    assert!(
        assistance.retention[0].contains("depuis 24 mois (12 mois à partir du 28/11/2026)"),
        "{}",
        assistance.retention[0]
    );
    let memory = activity(&register, ActivityKind::Memory);
    assert!(memory.retention[0].contains("supprimés 6 mois après"));
    assert!(memory.retention[1].contains("supprimées 7 jours après"));
}

#[test]
fn the_second_factor_measure_is_the_company_s_requirement() {
    let measure = |requirement| {
        let mut facts = bare();
        facts.second_factor = requirement;
        register(&facts)
            .measures
            .into_iter()
            .find(|m| m.starts_with("Second facteur"))
            .unwrap()
    };
    assert!(measure(Requirement::Optional).contains("exigé de personne"));
    assert!(measure(Requirement::Administrators).contains("exigé des administrateurs"));
    assert!(measure(Requirement::Everyone).contains("exigé de chaque compte"));
    assert!(
        !register(&bare())
            .measures
            .iter()
            .any(|m| m.contains("fournisseur d’identité"))
    );
}

#[test]
fn places_and_choices_have_stable_keys() {
    for place in [
        Place::EuropeanUnion,
        Place::OutsideEuropeanUnion,
        Place::ListedByOperator,
        Place::DeclaredByCompany,
        Place::Operator,
        Place::Unknown,
    ] {
        assert!(!place.key().is_empty() && !place.label().is_empty());
    }
    assert_eq!(ChosenBy::Company.key(), "company");
    assert_eq!(ChosenBy::Operator.key(), "operator");
}
