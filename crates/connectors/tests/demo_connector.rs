//! Demo interventions connector behind the policy enforcement point.

mod support;

use std::collections::HashSet;

use mimorya_agent::tools::ToolCallEvaluation;
use mimorya_connectors::demo_interventions::dates::monday_of;
use mimorya_core::demo::user;
use mimorya_core::tools::DeliverableInput;
use serde_json::json;
use support::{ConnectorHarness, executor, ids, reference, week};

#[test]
fn builds_the_week_around_the_reference_date() {
    assert_eq!(monday_of(reference()).to_string(), "2026-10-12");
}

#[tokio::test]
async fn scopes_search_results_to_each_users_permissions() {
    let (_, lucas) = ConnectorHarness::new(user::LUCAS)
        .call("interventions_search", week())
        .await;
    assert_eq!(ids(&lucas.unwrap()), ["INT-2041", "INT-2043", "INT-2046"]);

    let (_, julien) = ConnectorHarness::new(user::JULIEN)
        .call("interventions_search", week())
        .await;
    assert_eq!(
        ids(&julien.unwrap()),
        [
            "INT-2041", "INT-2042", "INT-2043", "INT-2044", "INT-2045", "INT-2046", "INT-2047"
        ]
    );

    let (_, marc) = ConnectorHarness::new(user::MARC)
        .call("interventions_search", week())
        .await;
    let marc = ids(&marc.unwrap());
    assert_eq!(marc.len(), 13);
    assert!(marc.contains(&"INT-2051".to_string())); // confidential, Marc has clearance

    let (_, claire) = ConnectorHarness::new(user::CLAIRE)
        .call("interventions_search", week())
        .await;
    assert_eq!(ids(&claire.unwrap()).len(), 13);
}

#[tokio::test]
async fn applies_abac_and_clearance_rules_per_record() {
    let (_, isabelle) = ConnectorHarness::new(user::ISABELLE)
        .call("interventions_search", week())
        .await;
    let isabelle = isabelle.unwrap();
    // Administrative staff: completed interventions only, and no confidential record.
    assert_eq!(
        ids(&isabelle),
        [
            "INT-2048", "INT-2041", "INT-2043", "INT-2044", "INT-2045", "INT-2050", "INT-2052"
        ]
    );
    let statuses: HashSet<&str> = isabelle.content["interventions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["statut"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, HashSet::from(["completed"]));
}

#[tokio::test]
async fn hides_the_tools_entirely_from_users_without_the_permission() {
    let (evaluation, _) = ConnectorHarness::new(user::PAUL)
        .call("interventions_search", week())
        .await;
    let ToolCallEvaluation::Deny { reason, .. } = evaluation else {
        panic!("expected a denial")
    };
    assert_eq!(reason, "Permission intervention:read requise");
}

#[tokio::test]
async fn does_not_reveal_whether_an_inaccessible_record_exists() {
    let lucas = ConnectorHarness::new(user::LUCAS);
    let (_, other) = lucas
        .call("interventions_get", json!({ "id": "INT-2048" }))
        .await;
    let (_, missing) = lucas
        .call("interventions_get", json!({ "id": "INT-9999" }))
        .await;
    assert_eq!(
        other.unwrap().content,
        json!({ "erreur": "Intervention INT-2048 introuvable ou non accessible" })
    );
    assert_eq!(
        missing.unwrap().content,
        json!({ "erreur": "Intervention INT-9999 introuvable ou non accessible" })
    );
}

#[tokio::test]
async fn flags_inconsistent_reports() {
    let (_, result) = ConnectorHarness::new(user::MARC)
        .call("interventions_get", json!({ "id": "INT-2043" }))
        .await;
    assert_eq!(
        result.unwrap().content["anomalies"],
        json!(["Statut « terminée » alors qu’une pièce est en attente (Circulateur Grundfos)"])
    );
}

fn slot(
    intervention: &str,
    technician: uuid::Uuid,
    date: &str,
    start: &str,
    reason: &str,
) -> serde_json::Value {
    json!({
        "interventionId": intervention,
        "technicianId": technician,
        "date": date,
        "startTime": start,
        "durationHours": 2,
        "reason": reason,
    })
}

#[tokio::test]
async fn validates_planning_proposals_and_creates_a_draft_deliverable() {
    let julien = ConnectorHarness::new(user::JULIEN);
    let (_, invalid) = julien
        .call(
            "planning_propose",
            json!({
                "title": "Suivi",
                "items": [
                    // Lucas is busy on Monday morning (INT-2061)
                    slot("INT-2043", user::LUCAS, "2026-10-19", "09:00", "Pose du circulateur"),
                    // Hugo belongs to the other team
                    slot("INT-2042", user::HUGO, "2026-10-20", "09:00", "Vanne"),
                    // Saturday
                    slot("INT-2046", user::EMMA, "2026-10-24", "09:00", "Compte rendu"),
                ],
            }),
        )
        .await;
    let invalid = invalid.unwrap();
    assert!(invalid.is_error);
    assert_eq!(
        invalid.content["problemes"],
        json!([
            "Créneau 1 (INT-2043) : chevauche « INT-2061 Entretien chaufferie »",
            "Créneau 2 (INT-2042) : technicien hors de votre périmètre de planification",
            "Créneau 3 (INT-2046) : 2026-10-24 n’est pas un jour ouvré",
        ])
    );

    let (_, valid) = julien
        .call(
            "planning_propose",
            json!({ "title": "Suivi semaine 43", "items": [slot("INT-2043", user::LUCAS, "2026-10-19", "13:00", "Pose du circulateur")] }),
        )
        .await;
    let valid = valid.unwrap();
    assert!(!valid.is_error);
    let id = valid.content["propositionId"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let deliverable = julien.ports.deliverable(id).unwrap();
    assert_eq!(deliverable.kind, "planning_proposal");
    assert_eq!(deliverable.status.as_str(), "draft");
}

#[tokio::test]
async fn requires_approval_to_apply_a_planning_then_updates_availability() {
    let julien = ConnectorHarness::new(user::JULIEN);
    let (_, proposal) = julien
        .call(
            "planning_propose",
            json!({ "title": "Suivi", "items": [slot("INT-2046", user::LUCAS, "2026-10-20", "08:00", "Compléter le compte rendu")] }),
        )
        .await;
    let proposal_id = proposal.unwrap().content["propositionId"]
        .as_str()
        .unwrap()
        .to_string();
    let (apply, _) = julien
        .call("planning_apply", json!({ "proposalId": proposal_id }))
        .await;
    let ToolCallEvaluation::RequireApproval { tool, input, .. } = apply else {
        panic!("expected an approval")
    };
    assert!(julien.store.applied().is_empty());

    // Executing as the approved action:
    let approved = executor(vec![tool.tool.clone()], user::JULIEN, &julien.ports);
    let (result, _) = approved.execute(&tool, input).await;
    assert!(!result.is_error, "{:?}", result.content);
    assert_eq!(julien.store.applied().len(), 1);
    assert_eq!(
        julien
            .ports
            .deliverable(proposal_id.parse().unwrap())
            .unwrap()
            .status
            .as_str(),
        "applied"
    );
}

#[tokio::test]
async fn refuses_to_apply_slots_outside_the_planning_apply_scope() {
    let julien = ConnectorHarness::new(user::JULIEN);
    // A proposal reaching beyond Julien's team, as a broader propose scope
    // (or a forged deliverable) could produce.
    let proposal = julien
        .ports
        .ports()
        .deliverables
        .create(DeliverableInput {
            kind: "planning_proposal".into(),
            title: "Suivi".into(),
            content: json!({
                "items": [
                    slot("INT-2043", user::LUCAS, "2026-10-20", "13:00", "Pose du circulateur"),
                    slot("INT-2042", user::HUGO, "2026-10-20", "09:00", "Vanne"),
                ],
            }),
            org_unit_id: None,
        })
        .await
        .unwrap();
    let tool = julien
        .executor
        .registry()
        .get("planning_apply")
        .unwrap()
        .clone();
    let (result, _) = julien
        .executor
        .execute(&tool, json!({ "proposalId": proposal.id.to_string() }))
        .await;
    assert!(result.is_error);
    assert!(
        result.content["erreur"]
            .as_str()
            .unwrap()
            .contains("INT-2042")
    );
    assert!(julien.store.applied().is_empty());
    assert_eq!(
        julien
            .ports
            .deliverable(proposal.id)
            .unwrap()
            .status
            .as_str(),
        "draft"
    );
}
