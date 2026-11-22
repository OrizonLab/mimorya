use super::{activity, bare, names, own};
use crate::processing_register::*;

#[test]
fn models_are_named_with_where_they_stand_and_who_chose_them() {
    let mut facts = bare();
    facts.runs_model = Some(own("anthropic"));
    facts.review_model = Some(ModelService {
        provider: "ollama".into(),
        server_host: Some("modeles.interne".into()),
        chosen_by: ChosenBy::Company,
    });
    facts.embeddings = Some(own("mistral"));
    let register = register(&facts);
    let assistance = activity(&register, ActivityKind::Assistance);
    assert_eq!(
        names(assistance),
        [
            "Anthropic",
            "Serveur de modèles désigné par l’exploitant (modeles.interne), API Ollama"
        ]
    );
    assert_eq!(assistance.recipients[0].place, Place::OutsideEuropeanUnion);
    assert_eq!(assistance.recipients[0].chosen_by, ChosenBy::Company);
    assert_eq!(assistance.recipients[1].place, Place::OperatorServer);
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
        enabled: true,
        kept: KeptData::default(),
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
        server_host: None,
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

/// How many times TypeSafe is named in the activity `kind`.
fn typesafe(register: &Register, kind: ActivityKind) -> usize {
    activity(register, kind)
        .recipients
        .iter()
        .filter(|r| r.name == "TypeSafe (Jev)" && r.place == Place::OutsideEuropeanUnion)
        .count()
}

#[test]
fn jev_receives_only_what_the_usages_set_send_it() {
    let mut facts = bare();
    facts.decisions = DecisionUses {
        memories: true,
        external_contents: true,
        ..DecisionUses::default()
    };
    facts.connectors = vec![ConnectorFact {
        kind: ConnectorKind::Rest,
        name: "Facturation".into(),
        hosts: vec!["api.facturation.example".into()],
        enabled: true,
        kept: KeptData::default(),
    }];
    // The main model decides: TypeSafe receives nothing.
    let register = super::register(&facts);
    for kind in [
        ActivityKind::Assistance,
        ActivityKind::Memory,
        ActivityKind::Documents,
        ActivityKind::Connectors,
    ] {
        assert_eq!(typesafe(&register, kind), 0);
    }
    facts.jev = true;
    let register = super::register(&facts);
    assert_eq!(typesafe(&register, ActivityKind::Assistance), 0);
    assert_eq!(typesafe(&register, ActivityKind::Memory), 1);
    // The beginning of what a tool brings, a deposited document's passages
    // as a connector's contents.
    assert_eq!(typesafe(&register, ActivityKind::Documents), 1);
    assert_eq!(typesafe(&register, ActivityKind::Connectors), 1);
    assert!(
        DecisionUses {
            requests: true,
            ..DecisionUses::default()
        }
        .any()
    );
    assert!(!DecisionUses::default().any());
}

#[test]
fn the_passages_go_to_jev_without_any_connector() {
    let mut facts = bare();
    facts.jev = true;
    facts.decisions.external_contents = true;
    let register = register(&facts);
    assert!(
        register
            .activities
            .iter()
            .all(|a| a.kind != ActivityKind::Connectors)
    );
    assert_eq!(typesafe(&register, ActivityKind::Documents), 1);
}

#[test]
fn the_main_model_receives_what_the_decisions_ask_it() {
    let mut facts = bare();
    facts.runs_model = Some(own("anthropic"));
    let role = |register: &Register, kind| activity(register, kind).recipients[0].role.clone();
    let register = register(&facts);
    assert!(!role(&register, ActivityKind::Memory).contains("données sensibles"));
    assert!(!role(&register, ActivityKind::Documents).contains("classement"));
    let memory = activity(&register, ActivityKind::Memory);
    assert!(memory.sensitive.unwrap().contains("ne les y cherche pas"));

    // Jev chosen or not, the main model decides what Jev does not.
    facts.decisions = DecisionUses {
        memories: true,
        documents: true,
        ..DecisionUses::default()
    };
    let register = super::register(&facts);
    assert!(
        role(&register, ActivityKind::Memory).contains("pour y chercher des données sensibles")
    );
    assert!(role(&register, ActivityKind::Documents).contains("pour son classement"));
    let memory = activity(&register, ActivityKind::Memory);
    assert!(
        memory
            .sensitive
            .unwrap()
            .contains("le service de décision les cherche")
    );
}
