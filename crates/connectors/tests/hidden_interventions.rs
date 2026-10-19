//! An intervention the user may not read stays hidden in the planning tools:
//! its slot shows as taken, without its identifier nor what it is.

mod support;

use chrono::Days;
use mimorya_connectors::demo_interventions::ScheduledSlot;
use mimorya_core::authz::{AbacPolicy, AuthorizationEngine, PolicyEffect};
use mimorya_core::demo::{self, user};
use serde_json::json;
use support::{ConnectorHarness, reference};
use uuid::Uuid;

/// The demo authorization, plus a deny on reading this intervention.
fn hiding(intervention: &str) -> AuthorizationEngine {
    let roles = demo::role_definitions()
        .iter()
        .map(demo::DemoRole::definition)
        .collect();
    let mut policies = demo::policies();
    policies.push(AbacPolicy {
        id: Uuid::new_v4(),
        description: "Masquée pour le test".into(),
        effect: PolicyEffect::Deny,
        permissions: vec!["intervention:read".into()],
        condition: serde_json::from_value(json!({
            "op": "eq", "left": { "attr": "resource.id" }, "right": { "value": intervention }
        }))
        .unwrap(),
    });
    AuthorizationEngine::new(demo::org_tree(), roles, policies)
}

#[tokio::test]
async fn availability_shows_an_unreadable_intervention_as_taken_only() {
    let monday = json!({ "from": "2026-10-12", "to": "2026-10-12" });
    let (_, visible) = ConnectorHarness::new(user::MARC)
        .call("team_availability", monday.clone())
        .await;
    assert!(
        visible
            .unwrap()
            .content
            .to_string()
            .contains("08:30-10:30 INT-2041")
    );

    let marc = ConnectorHarness::with_engine(user::MARC, hiding("INT-2041"));
    // A follow-up of the same intervention, applied earlier.
    marc.store.apply(vec![ScheduledSlot {
        intervention_id: "INT-2041".into(),
        technician_id: user::LUCAS,
        date: reference() - Days::new(2),
        start_time: "14:00".into(),
        duration_hours: 1.0,
        label: "Suivi INT-2041".into(),
    }]);
    let (_, hidden) = marc.call("team_availability", monday).await;
    let hidden = hidden.unwrap().content.to_string();
    assert!(!hidden.contains("INT-2041"), "{hidden}");
    for slot in ["08:30-10:30", "14:00-15:00"] {
        assert!(
            hidden.contains(&format!(
                "{slot} Occupé (intervention hors de votre périmètre)"
            )),
            "{hidden}"
        );
    }
}

#[tokio::test]
async fn a_clash_with_an_unreadable_intervention_does_not_name_it() {
    let julien = ConnectorHarness::with_engine(user::JULIEN, hiding("INT-2061"));
    let (_, refused) = julien
        .call(
            "planning_propose",
            json!({
                "title": "Suivi",
                "items": [{
                    "interventionId": "INT-2043",
                    "technicianId": user::LUCAS,
                    "date": "2026-10-19",
                    "startTime": "09:00",
                    "durationHours": 2,
                    "reason": "Pose du circulateur",
                }],
            }),
        )
        .await;
    let refused = refused.unwrap();
    assert!(refused.is_error);
    assert_eq!(
        refused.content["problemes"],
        json!([
            "Créneau 1 (INT-2043) : chevauche « Occupé (intervention hors de votre périmètre) »"
        ])
    );
}
