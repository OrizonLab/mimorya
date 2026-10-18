//! FICTIONAL demo company "Néo Maintenance" — example ABAC policy.

use serde_json::json;

use crate::authz::{AbacPolicy, PolicyEffect};

use super::ids::demo_id;

/// Administrative staff only sees completed interventions (billing).
pub fn policies() -> Vec<AbacPolicy> {
    let condition = serde_json::from_value(json!({
        "all": [
            { "op": "eq", "left": { "attr": "subject.attributes.department" }, "right": { "value": "administratif" } },
            { "op": "eq", "left": { "attr": "resource.type" }, "right": { "value": "intervention" } },
            { "op": "neq", "left": { "attr": "resource.attributes.status" }, "right": { "value": "completed" } }
        ]
    }))
    .expect("valid demo condition");
    vec![AbacPolicy {
        id: demo_id(400),
        description: "Le service administratif ne consulte que les interventions terminées".into(),
        effect: PolicyEffect::Deny,
        permissions: vec!["intervention:read".into()],
        condition,
    }]
}
