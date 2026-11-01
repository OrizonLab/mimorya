//! What an administrator changed of a connector's tool policies, as the
//! `connector.updated` audit keeps it (ADR-160): each tool whose policy
//! changed, with its new policy (`null` once removed), and its description
//! only when that changed (`null` once removed). Neither the policies left
//! as they were nor a description left as it was are written again: a
//! server's 256 KiB of descriptions would otherwise be copied at each change.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

/// The tools whose policy differs between `before` and `after`, each as
/// above; `None` when none does.
pub fn policy_changes(before: &Value, after: &Value) -> Option<Value> {
    let empty = Map::new();
    let before = before.as_object().unwrap_or(&empty);
    let after = after.as_object().unwrap_or(&empty);
    let tools: BTreeSet<&String> = before.keys().chain(after.keys()).collect();
    let mut changed = Map::new();
    for tool in tools {
        let (old, new) = (before.get(tool), after.get(tool));
        if old == new {
            continue;
        }
        let entry = match new {
            Some(new) => as_changed(old, new),
            None => Value::Null,
        };
        changed.insert(tool.clone(), entry);
    }
    (!changed.is_empty()).then_some(Value::Object(changed))
}

/// `new`, its description kept only when it differs from `old`'s.
fn as_changed(old: Option<&Value>, new: &Value) -> Value {
    let was = old.and_then(|old| old.get(DESCRIPTION));
    let is = new.get(DESCRIPTION);
    let mut entry = new.clone();
    if let Some(fields) = entry.as_object_mut() {
        match (was, is) {
            (was, is) if was == is => {
                fields.remove(DESCRIPTION);
            }
            (Some(_), None) => {
                fields.insert(DESCRIPTION.into(), Value::Null);
            }
            _ => {}
        }
    }
    entry
}

const DESCRIPTION: &str = "description";

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const LONG: &str = "Cherche une procédure de maintenance par mots-clés.";

    #[test]
    fn only_the_tools_whose_policy_changed_are_kept() {
        let before = json!({
            "a": { "risk": "read", "description": LONG },
            "b": { "risk": "read" },
            "c": { "enabled": false },
        });
        let after = json!({
            "a": { "risk": "write", "description": LONG },
            "b": { "risk": "read" },
            "d": { "risk": "read" },
        });
        assert_eq!(
            policy_changes(&before, &after),
            Some(json!({
                // Its description left as it was is not written again.
                "a": { "risk": "write" },
                "c": null,
                "d": { "risk": "read" },
            }))
        );
    }

    #[test]
    fn a_description_is_kept_when_written_changed_or_removed() {
        let before = json!({ "a": { "risk": "read", "description": LONG } });
        let rewritten = json!({ "a": { "risk": "read", "description": "Autre." } });
        assert_eq!(
            policy_changes(&before, &rewritten),
            Some(json!({ "a": { "risk": "read", "description": "Autre." } }))
        );
        let removed = json!({ "a": { "risk": "read" } });
        assert_eq!(
            policy_changes(&before, &removed),
            Some(json!({ "a": { "risk": "read", "description": null } }))
        );
        let written = json!({ "a": { "risk": "read", "description": LONG } });
        assert_eq!(
            policy_changes(&json!({ "a": { "risk": "read" } }), &written),
            Some(json!({ "a": { "risk": "read", "description": LONG } }))
        );
    }

    #[test]
    fn nothing_changed_is_nothing_kept() {
        let policies = json!({ "a": { "risk": "read", "description": LONG } });
        assert_eq!(policy_changes(&policies, &policies), None);
        assert_eq!(policy_changes(&json!({}), &json!({})), None);
        assert_eq!(policy_changes(&Value::Null, &json!({})), None);
    }
}
