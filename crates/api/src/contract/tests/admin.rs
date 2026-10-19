//! The administration's columns read as enumerations, from their reference
//! lists, and the tool policies it stores.

use std::collections::HashMap;

use mimorya_agent::tools::ToolOverride;
use mimorya_core::authz::Sensitivity as CoreSensitivity;
use mimorya_core::autonomy::ToolRisk;
use mimorya_db::repositories::access_policies::POLICY_EFFECTS;
use mimorya_db::repositories::audit::{AUDIT_ACTORS, AUDIT_OUTCOMES};
use mimorya_db::repositories::connectors::CONNECTOR_STATUSES;
use mimorya_db::repositories::skills::SKILL_STATUSES;
use mimorya_db::repositories::usage::USAGE_KINDS;
use mimorya_db::repositories::users::USER_STATUSES;
use serde_json::json;

use super::{every_value_reads_back, reads_back};
use crate::contract::admin;
use crate::contract::stored::{stored, stored_text};

#[test]
fn administration_columns_read_back_as_their_contract_values() {
    for status in USER_STATUSES {
        let read: admin::AccountStatus = stored_text("account status", status).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), status);
    }
    for clearance in CoreSensitivity::ALL {
        let read: admin::Sensitivity =
            stored_text("account clearance", clearance.as_str()).unwrap();
        assert_eq!(read, clearance.into());
        assert_eq!(serde_json::to_value(read).unwrap(), clearance.as_str());
    }
    for status in SKILL_STATUSES {
        let read: admin::SkillStatus = stored_text("skill status", status).unwrap();
        assert_eq!(serde_json::to_value(read).unwrap(), status);
    }
}

#[test]
fn activity_policy_and_connector_columns_read_back_as_their_contract_values() {
    every_value_reads_back::<admin::AuditActor>("audit actor", &AUDIT_ACTORS);
    every_value_reads_back::<admin::AuditOutcome>("audit outcome", &AUDIT_OUTCOMES);
    every_value_reads_back::<admin::UsageKind>("usage kind", &USAGE_KINDS);
    every_value_reads_back::<admin::PolicyEffect>("policy effect", &POLICY_EFFECTS);
    every_value_reads_back::<admin::ConnectorStatus>("connector status", &CONNECTOR_STATUSES);
}

/// The tool policies the administration stores (the engine's overrides)
/// read back as the contract declares them.
#[test]
fn stored_tool_policies_match_the_contract() {
    let written = HashMap::from([
        ("mcp_docs_search".to_string(), ToolOverride::default()),
        (
            "mcp_docs_write".to_string(),
            ToolOverride {
                enabled: Some(false),
                permissions: Some(vec!["intervention:read".into()]),
                risk: Some(ToolRisk::Write),
                requires_approval: Some(true),
            },
        ),
    ]);
    reads_back::<_, HashMap<String, admin::ToolPolicy>>(&written);
    // The listing returns them as stored: fields left out stay readable.
    let partial = json!({ "mcp_docs_search": { "enabled": false } });
    assert!(stored::<HashMap<String, admin::ToolPolicy>>("tool policies", &partial).is_ok());
}
