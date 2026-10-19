//! What an administrator may write about an MCP connector (ADR-025): a short
//! prefix naming its tools, an address the server may reach, an access token
//! fit for an HTTP header, and per-tool policies that only narrow access with
//! business permissions an agent can hold.

use std::collections::{BTreeSet, HashMap};

use mimorya_agent::tools::ToolOverride;
use mimorya_core::DomainError;
use mimorya_core::authz::PermissionInfo;
use mimorya_core::autonomy::ToolRisk;
use serde::Deserialize;
use serde_json::Value;

use crate::services::connectors::CONNECTOR_USE;

pub const MIN_TOKEN_CHARS: usize = 8;
pub const MAX_TOKEN_CHARS: usize = 2000;
pub const MAX_TOOL_POLICIES: usize = 200;
pub const MAX_TOOL_PERMISSIONS: usize = 10;

/// One tool's policy as an administrator writes it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolPolicyInput {
    pub enabled: Option<bool>,
    pub risk: Option<ToolRisk>,
    pub permissions: Option<Vec<String>>,
}

/// A lowercase letter then letters or digits (2 to 16): the tools are named
/// `mcp_<prefix>_<tool>`.
pub fn prefix_problem(prefix: &str) -> Option<String> {
    let mut chars = prefix.chars();
    let well_formed = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && (2..=16).contains(&prefix.len());
    (!well_formed).then(|| {
        "Préfixe invalide : une minuscule puis des minuscules ou des chiffres (2 à 16)".into()
    })
}

/// Sent as `Authorization: Bearer …`: visible ASCII only.
pub fn token_problem(token: &str) -> Option<String> {
    let length = token.chars().count();
    if !(MIN_TOKEN_CHARS..=MAX_TOKEN_CHARS).contains(&length) {
        return Some(format!(
            "Jeton de {MIN_TOKEN_CHARS} à {MAX_TOKEN_CHARS} caractères"
        ));
    }
    (!token.chars().all(|c| c.is_ascii_graphic()))
        .then(|| "Jeton invalide : caractères visibles, sans espace".into())
}

/// The policies to store, or why they are refused (in French). Each names a
/// tool of this instance (`mcp_<prefix>_…`); permissions are catalog
/// permissions an agent may hold, and `connector:use` is always kept.
pub fn checked_policies(
    policies: &HashMap<String, ToolPolicyInput>,
    prefix: &str,
    catalog: &[PermissionInfo],
) -> Result<Value, DomainError> {
    if policies.len() > MAX_TOOL_POLICIES {
        return Err(DomainError::validation(format!(
            "{MAX_TOOL_POLICIES} outils au plus"
        )));
    }
    let namespace = format!("mcp_{prefix}_");
    let mut stored = HashMap::new();
    for (name, policy) in policies {
        let well_formed = name.starts_with(&namespace)
            && name.len() > namespace.len()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !well_formed {
            return Err(DomainError::validation(format!(
                "Outil inconnu de ce connecteur : {name}"
            )));
        }
        let permissions = match &policy.permissions {
            Some(requested) => Some(checked_permissions(requested, catalog)?),
            None => None,
        };
        stored.insert(
            name.clone(),
            ToolOverride {
                enabled: policy.enabled,
                permissions,
                risk: policy.risk,
                requires_approval: None,
            },
        );
    }
    serde_json::to_value(stored).map_err(|_| DomainError::validation("Politiques invalides"))
}

fn checked_permissions(
    requested: &[String],
    catalog: &[PermissionInfo],
) -> Result<Vec<String>, DomainError> {
    let mut permissions: BTreeSet<String> =
        requested.iter().map(|p| p.trim().to_string()).collect();
    if permissions.len() > MAX_TOOL_PERMISSIONS {
        return Err(DomainError::validation(format!(
            "{MAX_TOOL_PERMISSIONS} permissions au plus par outil"
        )));
    }
    for permission in &permissions {
        // Only what an agent can be delegated: an administration permission
        // could never be used by the agent anyway.
        if !catalog
            .iter()
            .any(|p| p.permission == permission && p.delegable)
        {
            return Err(DomainError::validation(format!(
                "Permission inconnue ou non délégable à l’agent : {permission}"
            )));
        }
    }
    permissions.insert(CONNECTOR_USE.into());
    Ok(permissions.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use mimorya_core::authz::Enforcement;
    use serde_json::json;

    use super::*;

    const CATALOG: &[PermissionInfo] = &[
        PermissionInfo {
            permission: "connector:use",
            label: "",
            delegable: true,
            enforcement: Enforcement::Holder,
        },
        PermissionInfo {
            permission: "intervention:read",
            label: "",
            delegable: true,
            enforcement: Enforcement::PerResource,
        },
        PermissionInfo {
            permission: "user:manage",
            label: "",
            delegable: false,
            enforcement: Enforcement::Holder,
        },
    ];

    fn policies(value: Value) -> HashMap<String, ToolPolicyInput> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn keeps_connector_use_and_scopes_names_to_the_instance() {
        let stored = checked_policies(
            &policies(json!({ "mcp_docs_search": { "risk": "read", "permissions": ["intervention:read"] } })),
            "docs",
            CATALOG,
        )
        .unwrap();
        assert_eq!(
            stored["mcp_docs_search"]["permissions"],
            json!(["connector:use", "intervention:read"])
        );
        assert_eq!(stored["mcp_docs_search"]["risk"], "read");
        for (refused, prefix) in [
            (json!({ "mcp_other_search": {} }), "docs"),
            (
                json!({ "interventions_search": { "enabled": false } }),
                "docs",
            ),
            (json!({ "mcp_docs_": {} }), "docs"),
            (
                json!({ "mcp_docs_search": { "permissions": ["user:manage"] } }),
                "docs",
            ),
            (
                json!({ "mcp_docs_search": { "permissions": ["facture:read"] } }),
                "docs",
            ),
        ] {
            assert!(
                checked_policies(&policies(refused.clone()), prefix, CATALOG).is_err(),
                "{refused}"
            );
        }
        assert!(
            serde_json::from_value::<HashMap<String, ToolPolicyInput>>(
                json!({ "mcp_docs_search": { "requiresApproval": false } })
            )
            .is_err()
        );
    }

    #[test]
    fn checks_prefixes_and_tokens() {
        assert_eq!(prefix_problem("docs2"), None);
        for refused in ["d", "Docs", "2docs", "docs_x", "abcdefghijklmnopq"] {
            assert!(prefix_problem(refused).is_some(), "{refused}");
        }
        assert_eq!(token_problem("jeton-0123456789"), None);
        for refused in ["court", "avec espace 123", "accentu\u{e9}-123"] {
            assert!(token_problem(refused).is_some(), "{refused}");
        }
    }
}
