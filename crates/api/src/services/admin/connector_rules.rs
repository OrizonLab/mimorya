//! What an administrator may write about an MCP connector (ADR-025): a short
//! prefix naming its tools and per-tool policies that only narrow access with
//! business permissions an agent can hold. The address and the token follow
//! the rules of `connectors::mcp` (`url_problem`, `token_problem`).

use std::collections::{BTreeSet, HashMap};

use mimorya_agent::tools::ToolOverride;
use mimorya_connectors::mcp::{
    CONNECTOR_USE, MAX_DECLARATIONS_BYTES, MAX_DESCRIPTION_CHARS, MAX_TOOLS, NetworkPolicy,
    token_problem, tool_namespace, url_problem,
};
use mimorya_core::DomainError;
use mimorya_core::authz::PermissionInfo;
use mimorya_core::autonomy::ToolRisk;
use mimorya_core::plain_text::shows_all_it_holds;
use serde_json::Value;

/// One policy per tool a server may declare.
pub const MAX_TOOL_POLICIES: usize = MAX_TOOLS;
pub const MAX_TOOL_PERMISSIONS: usize = 10;

/// One tool's policy as an administrator writes it (read from the API's
/// `ToolPolicyChange`).
#[derive(Debug, Clone)]
pub struct ToolPolicyInput {
    pub enabled: Option<bool>,
    pub risk: Option<ToolRisk>,
    pub permissions: Option<Vec<String>>,
    /// Replaces what the model reads of the tool (ADR-057).
    pub description: Option<String>,
}

/// Why an MCP server may not be added as typed, if it may not: its prefix,
/// its address for this platform, the format of its token. An addition and a
/// trial (ADR-050) are checked alike, before any connection.
pub fn mcp_addition_problem(
    prefix: &str,
    url: &str,
    token: Option<&str>,
    network: NetworkPolicy,
) -> Option<String> {
    prefix_problem(prefix)
        .or_else(|| url_problem(url, network))
        .or_else(|| token.and_then(token_problem))
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

/// The policies to store, or why they are refused (in French). Each names a
/// tool of this instance (`mcp_<prefix>_…`); permissions are catalog
/// permissions an agent may hold, and `connector:use` is always kept; a
/// replacement description is bounded like a server's own, and so are all of
/// them together.
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
    let namespace = tool_namespace(prefix);
    let mut stored = HashMap::new();
    let mut described_bytes = 0;
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
        let description = match &policy.description {
            Some(text) => checked_description(name, text)?,
            None => None,
        };
        described_bytes += description.as_ref().map_or(0, String::len);
        if described_bytes > MAX_DECLARATIONS_BYTES {
            return Err(DomainError::validation(format!(
                "Descriptions remplacées : {} Kio au plus pour l’ensemble des outils",
                MAX_DECLARATIONS_BYTES / 1024
            )));
        }
        stored.insert(
            name.clone(),
            ToolOverride {
                enabled: policy.enabled,
                permissions,
                risk: policy.risk,
                requires_approval: None,
                description,
            },
        );
    }
    serde_json::to_value(stored).map_err(|_| DomainError::validation("Politiques invalides"))
}

/// What the model will read of the tool instead of the server's description
/// (none if blank): as long as a server's at most, and showing all it holds,
/// line breaks and tabs aside, as a skill's content (ADR-052).
fn checked_description(tool: &str, text: &str) -> Result<Option<String>, DomainError> {
    let text = text.trim().replace("\r\n", "\n");
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(DomainError::validation(format!(
            "Description de {tool} : {MAX_DESCRIPTION_CHARS} caractères au plus"
        )));
    }
    if !shows_all_it_holds(&text) {
        return Err(DomainError::validation(format!(
            "Description de {tool} : du texte visible, des retours à la ligne et des \
             tabulations seulement"
        )));
    }
    Ok(Some(text))
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

    use crate::contract::admin::ToolPolicyChange;

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
        let read: HashMap<String, ToolPolicyChange> = serde_json::from_value(value).unwrap();
        read.into_iter()
            .map(|(tool, policy)| (tool, policy.into()))
            .collect()
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
    }

    #[test]
    fn keeps_a_replaced_description_within_a_server_s_bounds() {
        let described = |text: &str| {
            checked_policies(
                &policies(json!({ "mcp_docs_search": { "description": text } })),
                "docs",
                CATALOG,
            )
        };
        let stored = described("  Cherche une procédure.\r\nRéponse : une liste.  ").unwrap();
        assert_eq!(
            stored["mcp_docs_search"]["description"],
            "Cherche une procédure.\nRéponse : une liste."
        );
        // Blank (after trimming), it is no replacement.
        let stored = described(" ").unwrap();
        assert_eq!(stored["mcp_docs_search"]["description"], Value::Null);
        assert!(described(&"é".repeat(MAX_DESCRIPTION_CHARS)).is_ok());
        assert!(described(&"é".repeat(MAX_DESCRIPTION_CHARS + 1)).is_err());
        assert!(described("Colonnes\tséparées").is_ok());
        // Controls, and what hides in a line: zero-width space, direction
        // override, tag characters (hidden text), line separator, BOM.
        for refused in [
            "Fin\u{0}",
            "Effacé\u{8}",
            "Saut\rseul",
            "Échappé\u{1b}[2J",
            "Sans\u{200b}largeur",
            "Inversé\u{202e}",
            "Caché\u{e0041}\u{e0042}",
            "Ligne\u{2028}séparée",
            "\u{feff}Marque",
        ] {
            assert!(
                checked_description("mcp_docs_search", refused).is_err(),
                "{refused:?}"
            );
        }
        // A null character does not even get past the request body.
        let body = json!({ "description": "Fin\u{0}" });
        assert!(serde_json::from_value::<ToolPolicyChange>(body).is_err());
    }

    #[test]
    fn bounds_all_the_replaced_descriptions_together() {
        // 2 bytes a character: 100 full descriptions fit, 150 do not.
        let described = |tools: usize| {
            let all: serde_json::Map<String, Value> = (0..tools)
                .map(|i| {
                    let text = "é".repeat(MAX_DESCRIPTION_CHARS);
                    (format!("mcp_docs_tool{i}"), json!({ "description": text }))
                })
                .collect();
            checked_policies(&policies(Value::Object(all)), "docs", CATALOG)
        };
        assert!(described(100).is_ok());
        assert!(described(150).is_err());
    }

    #[test]
    fn checks_prefixes() {
        assert_eq!(prefix_problem("docs2"), None);
        for refused in ["d", "Docs", "2docs", "docs_x", "abcdefghijklmnopq"] {
            assert!(prefix_problem(refused).is_some(), "{refused}");
        }
    }
}
