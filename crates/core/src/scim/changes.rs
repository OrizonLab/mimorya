//! What a change the identity provider sends to an account sets (ADR-183):
//! a replacement (`PUT`) or a patch's operations (`PATCH`, RFC 7644 §3.5.2),
//! as Entra ID and Okta send them. Kept: the name (`displayName`), the job
//! title (`title`, which may be taken away), the provider's identifier
//! (`externalId`, likewise) and whether the account is active. The address
//! (`userName`) is read to be compared, never changed: given another, the
//! change is refused. Anything else is read by no one, as at creation.

use serde_json::{Map, Value};

use super::user::{
    NUL, ScimUserFields, check_name, external_id_of, name_given, refuse_nul, title_of,
};

/// What a change sets; each field `None` when the change leaves it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScimChanges {
    /// The address the change names, lowercase: compared, never changed.
    pub user_name: Option<String>,
    pub display_name: Option<String>,
    /// `Some(None)` takes the job title away.
    pub title: Option<Option<String>>,
    /// `Some(None)` takes the provider's identifier away.
    pub external_id: Option<Option<String>>,
    pub active: Option<bool>,
}

/// Why a change is refused, as SCIM names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeRefused {
    /// The request is not a change SCIM reads (`invalidSyntax`).
    Syntax(String),
    /// A value the account could not hold (`invalidValue`).
    Value(String),
    /// An attribute that may not change or go (`mutability`).
    Mutability(String),
}

/// What a replacement (`PUT`) sets: the kept attributes it sends; those it
/// leaves out stay as they are.
pub fn replacement(fields: ScimUserFields) -> Result<ScimChanges, ChangeRefused> {
    refuse_nul(&fields).map_err(ChangeRefused::Value)?;
    let display_name = name_given(&fields);
    if let Some(name) = &display_name {
        check_name(name).map_err(ChangeRefused::Value)?;
    }
    Ok(ScimChanges {
        user_name: fields.user_name.map(|name| name.trim().to_lowercase()),
        display_name,
        title: match fields.title {
            Some(title) => Some(title_of(Some(&title)).map_err(ChangeRefused::Value)?),
            None => None,
        },
        external_id: match fields.external_id {
            Some(id) => Some(external_id_of(Some(id)).map_err(ChangeRefused::Value)?),
            None => None,
        },
        active: fields.active,
    })
}

/// What the operations of a patch set, applied in their order.
pub fn patch(operations: &[Value]) -> Result<ScimChanges, ChangeRefused> {
    if operations.is_empty() {
        return Err(syntax("Operations : au moins une opération"));
    }
    let mut changes = ScimChanges::default();
    for operation in operations {
        let operation = operation
            .as_object()
            .ok_or_else(|| syntax("Chaque opération est un objet"))?;
        let op = text(operation, "op")
            .ok_or_else(|| syntax("op : add, replace ou remove"))?
            .to_ascii_lowercase();
        let path = text(operation, "path").map(attribute_of);
        match (op.as_str(), path) {
            ("add" | "replace", Some(path)) => {
                let value = field(operation, "value").unwrap_or(&Value::Null);
                set(&mut changes, &path, value)?;
            }
            ("add" | "replace", None) => {
                let values = field(operation, "value")
                    .and_then(Value::as_object)
                    .ok_or_else(|| syntax("value : un objet, sans path"))?;
                for (name, value) in values {
                    set(&mut changes, &attribute_of(name), value)?;
                }
            }
            ("remove", Some(path)) => remove(&mut changes, &path)?,
            ("remove", None) => return Err(syntax("remove : un path, obligatoire")),
            _ => return Err(syntax("op : add, replace ou remove")),
        }
    }
    Ok(changes)
}

/// The attribute `path` names, lowercase, without the core schema's prefix.
fn attribute_of(path: &str) -> String {
    const CORE: &str = "urn:ietf:params:scim:schemas:core:2.0:user:";
    let path = path.trim().to_ascii_lowercase();
    match path.strip_prefix(CORE) {
        Some(attribute) => attribute.to_string(),
        None => path,
    }
}

/// Sets what `attribute` (lowercase) names to `value`; an attribute not
/// kept is read by no one.
fn set(changes: &mut ScimChanges, attribute: &str, value: &Value) -> Result<(), ChangeRefused> {
    match attribute {
        "username" => changes.user_name = Some(required(value, "userName")?.to_lowercase()),
        "displayname" => {
            let name = required(value, "displayName")?;
            check_name(&name).map_err(ChangeRefused::Value)?;
            changes.display_name = Some(name);
        }
        "title" => {
            let title = optional(value, "title")?;
            changes.title = Some(title_of(title.as_deref()).map_err(ChangeRefused::Value)?);
        }
        "externalid" => {
            let id = optional(value, "externalId")?;
            changes.external_id = Some(external_id_of(id).map_err(ChangeRefused::Value)?);
        }
        "active" => changes.active = Some(flag(value)?),
        _ => {}
    }
    Ok(())
}

/// Takes away what `attribute` (lowercase) names, if it may go.
fn remove(changes: &mut ScimChanges, attribute: &str) -> Result<(), ChangeRefused> {
    match attribute {
        "title" => changes.title = Some(None),
        "externalid" => changes.external_id = Some(None),
        "username" | "displayname" | "active" => {
            return Err(ChangeRefused::Mutability(format!(
                "{attribute} : ne se retire pas"
            )));
        }
        _ => {}
    }
    Ok(())
}

/// The text `value` holds, trimmed, never empty.
fn required(value: &Value, name: &str) -> Result<String, ChangeRefused> {
    match optional(value, name)? {
        Some(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        _ => Err(ChangeRefused::Value(format!("{name} : obligatoire"))),
    }
}

/// The text `value` holds, or none when it is null.
fn optional(value: &Value, name: &str) -> Result<Option<String>, ChangeRefused> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) if text.contains('\0') => Err(ChangeRefused::Value(NUL.into())),
        Value::String(text) => Ok(Some(text.clone())),
        _ => Err(ChangeRefused::Value(format!(
            "{name} : une chaîne de caractères"
        ))),
    }
}

/// `true`, `false`, or their strings in any case (`"False"`, as Entra ID
/// sends it).
fn flag(value: &Value) -> Result<bool, ChangeRefused> {
    match value {
        Value::Bool(value) => Ok(*value),
        Value::String(text) if text.eq_ignore_ascii_case("true") => Ok(true),
        Value::String(text) if text.eq_ignore_ascii_case("false") => Ok(false),
        _ => Err(ChangeRefused::Value("active : true ou false".into())),
    }
}

/// The member `key` of `object`, its name compared without case.
fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

fn text<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    field(object, key).and_then(Value::as_str)
}

fn syntax(detail: &str) -> ChangeRefused {
    ChangeRefused::Syntax(detail.into())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn patched(operations: Value) -> Result<ScimChanges, ChangeRefused> {
        patch(operations.as_array().unwrap())
    }

    #[test]
    fn reads_the_operations_entra_id_sends() {
        let changes = patched(json!([
            { "op": "Replace", "path": "displayName", "value": " Léa Martin " },
            { "op": "Replace", "path": "title", "value": "Cheffe d’équipe" },
            { "op": "Add", "path": "externalId", "value": "lmartin" },
            { "op": "Replace", "path": "active", "value": "False" },
            { "op": "Add", "path": "phoneNumbers[type eq \"work\"].value", "value": "0102" },
            { "op": "Replace", "path": "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User:department", "value": "Lyon" },
            { "op": "Replace", "path": "name.givenName", "value": "Léa" }
        ]));
        assert_eq!(
            changes,
            Ok(ScimChanges {
                user_name: None,
                display_name: Some("Léa Martin".into()),
                title: Some(Some("Cheffe d’équipe".into())),
                external_id: Some(Some("lmartin".into())),
                active: Some(false),
            })
        );
    }

    #[test]
    fn reads_the_operations_okta_sends() {
        let changes = patched(json!([
            { "op": "replace", "value": { "active": false, "userName": "Lea@Neo.example" } }
        ]));
        assert_eq!(
            changes,
            Ok(ScimChanges {
                user_name: Some("lea@neo.example".into()),
                active: Some(false),
                ..ScimChanges::default()
            })
        );
        let core = "urn:ietf:params:scim:schemas:core:2.0:User:title";
        assert_eq!(
            patched(
                json!([{ "op": "remove", "path": core }, { "op": "remove", "path": "externalId" }])
            ),
            Ok(ScimChanges {
                title: Some(None),
                external_id: Some(None),
                ..ScimChanges::default()
            })
        );
        // The last operation on an attribute stands.
        assert_eq!(
            patched(json!([
                { "op": "replace", "path": "active", "value": false },
                { "op": "replace", "path": "active", "value": true }
            ]))
            .map(|changes| changes.active),
            Ok(Some(true))
        );
    }

    #[test]
    fn refuses_what_cannot_be_read_or_changed() {
        for (operations, refused) in [
            (json!([]), "Syntax"),
            (json!(["replace"]), "Syntax"),
            (json!([{ "path": "title", "value": "x" }]), "Syntax"),
            (json!([{ "op": "move", "path": "title" }]), "Syntax"),
            (json!([{ "op": "remove" }]), "Syntax"),
            (json!([{ "op": "replace", "value": "Léa" }]), "Syntax"),
            (
                json!([{ "op": "replace", "path": "active", "value": "oui" }]),
                "Value",
            ),
            (
                json!([{ "op": "replace", "path": "displayName", "value": "L" }]),
                "Value",
            ),
            (
                json!([{ "op": "replace", "path": "displayName", "value": null }]),
                "Value",
            ),
            (
                json!([{ "op": "replace", "path": "title", "value": 3 }]),
                "Value",
            ),
            (
                json!([{ "op": "replace", "path": "title", "value": "t".repeat(121) }]),
                "Value",
            ),
            (
                json!([{ "op": "replace", "path": "externalId", "value": "a\u{0}" }]),
                "Value",
            ),
            (
                json!([{ "op": "remove", "path": "userName" }]),
                "Mutability",
            ),
            (
                json!([{ "op": "remove", "path": "displayName" }]),
                "Mutability",
            ),
            (json!([{ "op": "remove", "path": "active" }]), "Mutability"),
        ] {
            let kind = match patched(operations.clone()) {
                Err(ChangeRefused::Syntax(_)) => "Syntax",
                Err(ChangeRefused::Value(_)) => "Value",
                Err(ChangeRefused::Mutability(_)) => "Mutability",
                Ok(_) => "accepted",
            };
            assert_eq!(kind, refused, "{operations}");
        }
    }

    #[test]
    fn a_replacement_leaves_what_it_does_not_send() {
        let changes = replacement(ScimUserFields {
            user_name: Some(" Lea@Neo.example".into()),
            given_name: Some("Léa".into()),
            family_name: Some("Martin".into()),
            title: Some("".into()),
            active: Some(true),
            ..ScimUserFields::default()
        });
        assert_eq!(
            changes,
            Ok(ScimChanges {
                user_name: Some("lea@neo.example".into()),
                display_name: Some("Léa Martin".into()),
                title: Some(None),
                external_id: None,
                active: Some(true),
            })
        );
        assert!(matches!(
            replacement(ScimUserFields {
                display_name: Some("L".into()),
                ..ScimUserFields::default()
            }),
            Err(ChangeRefused::Value(_))
        ));
    }
}
