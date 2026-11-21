//! The register's unit tests: what a company without services has, its
//! durations, its measures; its models and decisions (`models.rs`); what
//! reaches out of the platform (`reach.rs`).

mod models;
mod reach;

use chrono::{TimeZone, Utc};

use super::*;
use crate::retention::RetentionKind;
use crate::second_factor::Requirement;

pub(super) fn bare() -> RegisterFacts {
    RegisterFacts {
        company: "Néo Maintenance".into(),
        runs_model: None,
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
    }
}

pub(super) fn own(provider: &str) -> ModelService {
    ModelService {
        provider: provider.into(),
        server_host: None,
        chosen_by: ChosenBy::Company,
    }
}

pub(super) fn activity(register: &Register, kind: ActivityKind) -> &Activity {
    register
        .activities
        .iter()
        .find(|activity| activity.kind == kind)
        .expect("the activity")
}

pub(super) fn names(activity: &Activity) -> Vec<&str> {
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
    assert_eq!(register.to_complete.len(), 7);
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
        Place::OperatorServer,
        Place::DeclaredByCompany,
        Place::Operator,
        Place::Unknown,
    ] {
        assert!(!place.key().is_empty() && !place.label().is_empty());
    }
    assert_eq!(ChosenBy::Company.key(), "company");
    assert_eq!(ChosenBy::Operator.key(), "operator");
}
