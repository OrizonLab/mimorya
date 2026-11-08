//! What the identity provider sends as an account (`POST /scim/v2/Users`,
//! ADR-183), read leniently, as SCIM asks: attribute names in any case
//! (RFC 7643 §2.1), anything this server does not keep ignored, never
//! stored, `active` as a boolean or as the string Entra ID sends. Unlike the
//! application's bodies, it is not strict: a provider sends every attribute
//! its mapping holds, and a strict body would refuse its accounts. A kept
//! attribute of the wrong type is refused.

use mimorya_core::scim::ScimUserFields;
use serde_json::{Map, Value};

/// The kept attributes of the account `body` describes, or why they cannot
/// be read.
pub fn read_user_fields(body: &Value) -> Result<ScimUserFields, String> {
    let object = body.as_object().ok_or("Le corps doit être un objet JSON")?;
    let name = match attribute(object, "name") {
        None | Some(Value::Null) => None,
        Some(Value::Object(name)) => Some(name),
        Some(_) => return Err("name : un objet".into()),
    };
    let in_name = |key: &str| match name {
        Some(name) => text(name, key, "name."),
        None => Ok(None),
    };
    Ok(ScimUserFields {
        user_name: text(object, "userName", "")?,
        display_name: text(object, "displayName", "")?,
        formatted_name: in_name("formatted")?,
        given_name: in_name("givenName")?,
        family_name: in_name("familyName")?,
        title: text(object, "title", "")?,
        external_id: text(object, "externalId", "")?,
        active: flag(object, "active")?,
    })
}

/// The operations of a patch (`PATCH /scim/v2/Users/{id}`), its
/// `Operations` named in any case; read one by one by `core::scim::patch`.
pub fn read_operations(body: &Value) -> Result<&[Value], String> {
    body.as_object()
        .and_then(|object| attribute(object, "Operations"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| "Operations : la liste des opérations, obligatoire".into())
}

/// The attribute `key` of `object`, its name compared without case.
fn attribute<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

/// The string attribute `key`, absent when not sent or null.
fn text(object: &Map<String, Value>, key: &str, path: &str) -> Result<Option<String>, String> {
    match attribute(object, key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(format!("{path}{key} : une chaîne de caractères")),
    }
}

/// The boolean attribute `key`: `true`, `false`, or their strings in any
/// case (`"True"`, as Entra ID sends it).
pub fn flag(object: &Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match attribute(object, key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(Value::String(value)) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(format!("{key} : true ou false")),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn reads_an_account_as_entra_id_sends_it() {
        let sent = json!({
            "schemas": [
                "urn:ietf:params:scim:schemas:core:2.0:User",
                "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User"
            ],
            "externalId": "lmartin",
            "UserName": "Lea.Martin@neo.example",
            "active": "True",
            "displayName": "Léa Martin",
            "emails": [{ "primary": true, "type": "work", "value": "Lea.Martin@neo.example" }],
            "name": { "Formatted": "Léa Martin", "familyName": "Martin", "givenName": "Léa" },
            "title": "Technicienne",
            "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User": { "department": "Lyon" }
        });
        assert_eq!(
            read_user_fields(&sent),
            Ok(ScimUserFields {
                user_name: Some("Lea.Martin@neo.example".into()),
                display_name: Some("Léa Martin".into()),
                formatted_name: Some("Léa Martin".into()),
                given_name: Some("Léa".into()),
                family_name: Some("Martin".into()),
                title: Some("Technicienne".into()),
                external_id: Some("lmartin".into()),
                active: Some(true),
            })
        );
        assert_eq!(
            read_user_fields(&json!({ "userName": "a@b.example", "name": null, "title": null })),
            Ok(ScimUserFields {
                user_name: Some("a@b.example".into()),
                ..ScimUserFields::default()
            })
        );
    }

    #[test]
    fn refuses_a_kept_attribute_of_the_wrong_type() {
        for sent in [
            json!([]),
            json!("a@b.example"),
            json!({ "userName": 5 }),
            json!({ "userName": "a@b.example", "active": "oui" }),
            json!({ "userName": "a@b.example", "active": 1 }),
            json!({ "userName": "a@b.example", "name": "Léa" }),
            json!({ "userName": "a@b.example", "name": { "givenName": ["Léa"] } }),
            json!({ "userName": "a@b.example", "externalId": 42 }),
        ] {
            assert!(read_user_fields(&sent).is_err(), "{sent}");
        }
    }
}
