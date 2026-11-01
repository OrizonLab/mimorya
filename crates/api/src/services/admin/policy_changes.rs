//! What an administrator changed of a connector's tool policies, as the
//! `connector.updated` audit keeps it (ADR-160): each tool whose policy
//! changed, with its new policy (`null` once removed), and its description
//! only when that changed: its text once written, `null` once none (a
//! description is stored `null` when there is none). Neither the policies
//! left as they were nor a description left as it was are written again:
//! saving the tool rules sends them all, up to a server's 256 KiB.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

/// The tools whose policy differs between `before` and `after`, each as
/// above; `None` when none does. Policies are an object, `{}` by default,
/// which the administration alone writes: anything else reads as none.
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

/// `new`, its description kept only when it differs from `old`'s: absent
/// and `null` both say there is none.
fn as_changed(old: Option<&Value>, new: &Value) -> Value {
    let none = Value::Null;
    let was = old.and_then(|old| old.get(DESCRIPTION)).unwrap_or(&none);
    let is = new.get(DESCRIPTION).unwrap_or(&none).clone();
    let mut entry = new.clone();
    if let Some(fields) = entry.as_object_mut() {
        if *was == is {
            fields.remove(DESCRIPTION);
        } else {
            fields.insert(DESCRIPTION.into(), is);
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

    /// A policy as stored: its five fields, `null` when not set.
    fn stored(risk: &str, enabled: Option<bool>, description: Option<&str>) -> Value {
        json!({
            "enabled": enabled,
            "permissions": ["connector:use", "intervention:read"],
            "risk": risk,
            "requiresApproval": null,
            "description": description,
        })
    }

    /// The same, without its description: as audited when it did not change.
    fn audited(risk: &str, enabled: Option<bool>) -> Value {
        let mut policy = stored(risk, enabled, None);
        policy.as_object_mut().unwrap().remove(DESCRIPTION);
        policy
    }

    #[test]
    fn only_the_tools_whose_policy_changed_are_kept() {
        let before = json!({
            "a": stored("read", None, Some(LONG)),
            "b": stored("read", None, None),
            "c": stored("read", Some(false), None),
        });
        let after = json!({
            "a": stored("write", None, Some(LONG)),
            "b": stored("read", None, None),
            "d": stored("read", None, None),
        });
        assert_eq!(
            policy_changes(&before, &after),
            Some(json!({
                // Its description left as it was is not written again.
                "a": audited("write", None),
                "c": null,
                // Mapped without a description: none to say.
                "d": audited("read", None),
            }))
        );
    }

    #[test]
    fn a_description_is_kept_when_written_changed_or_removed() {
        let before = json!({ "a": stored("read", None, Some(LONG)) });
        let rewritten = json!({ "a": stored("read", None, Some("Autre.")) });
        assert_eq!(
            policy_changes(&before, &rewritten),
            Some(json!({ "a": stored("read", None, Some("Autre.")) }))
        );
        let removed = json!({ "a": stored("read", None, None) });
        assert_eq!(
            policy_changes(&before, &removed),
            Some(json!({ "a": stored("read", None, None) }))
        );
        let written = json!({ "a": stored("read", None, Some(LONG)) });
        assert_eq!(
            policy_changes(&removed, &written),
            Some(json!({ "a": stored("read", None, Some(LONG)) }))
        );
        // An older row without the key reads as none.
        let older = json!({ "a": { "risk": "read" } });
        let disabled = json!({ "a": stored("read", Some(false), None) });
        let changes = policy_changes(&older, &disabled).unwrap();
        assert!(changes["a"].get(DESCRIPTION).is_none(), "{changes}");
    }

    #[test]
    fn nothing_changed_is_nothing_kept() {
        let policies = json!({ "a": stored("read", None, Some(LONG)) });
        assert_eq!(policy_changes(&policies, &policies), None);
        assert_eq!(policy_changes(&json!({}), &json!({})), None);
        assert_eq!(policy_changes(&Value::Null, &json!({})), None);
    }
}
