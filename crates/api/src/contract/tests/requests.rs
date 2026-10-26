//! Request bodies are strict: a body the web client may send (with only its
//! required fields, or with every field) is read, and the same body with an
//! unknown field is refused, so that a misnamed field cannot be ignored.
//! Every body of the contract's list refuses one (ADR-039, ADR-040).

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::contract::admin::*;
use crate::contract::*;

/// `minimal` and `complete` are read; `complete` with one more field is not.
fn strict<T: DeserializeOwned>(minimal: Value, complete: Value) {
    let name = std::any::type_name::<T>();
    serde_json::from_value::<T>(minimal).unwrap_or_else(|e| panic!("{name}: {e}"));
    let mut extra = complete.clone();
    serde_json::from_value::<T>(complete).unwrap_or_else(|e| panic!("{name}: {e}"));
    extra["inconnu"] = json!(true);
    assert!(
        serde_json::from_value::<T>(extra).is_err(),
        "{name} reads an unknown field"
    );
}

#[test]
fn session_and_link_bodies_refuse_unknown_fields() {
    let credentials = json!({ "email": "a@neo-maintenance.example", "password": "secret" });
    strict::<Credentials>(credentials.clone(), credentials);
    let update = json!({ "currentPassword": "ancien", "newPassword": "nouveau-mot-de-passe" });
    strict::<PasswordUpdate>(update.clone(), update);
    let email = json!({ "email": "a@neo-maintenance.example" });
    strict::<ResetLinkRequest>(email.clone(), email);
    strict::<LinkToken>(json!({ "token": "t" }), json!({ "token": "t" }));
    let chosen = json!({ "token": "t", "password": "nouveau-mot-de-passe" });
    strict::<LinkPassword>(chosen.clone(), chosen);
}

#[test]
fn conversation_approval_and_memory_bodies_refuse_unknown_fields() {
    strict::<NewConversation>(json!({}), json!({ "title": "Planning" }));
    strict::<ConversationChanges>(json!({}), json!({ "title": "Planning", "archived": true }));
    strict::<NewMessage>(
        json!({ "content": "Bonjour" }),
        json!({ "content": "Bonjour" }),
    );
    strict::<ApprovalVerdict>(
        json!({ "approved": true }),
        json!({ "approved": false, "comment": "Pas ce créneau" }),
    );
    let memory = json!({
        "space": "personal",
        "kind": "preference",
        "title": "Rapports",
        "content": "Rapports en PDF",
    });
    strict::<ManualMemory>(memory.clone(), memory);
    let review = json!({ "decision": "approve" });
    strict::<MemoryReview>(review.clone(), review);
}

/// Read alone, an unknown field is refused before any missing one.
fn refuses_unknown<T: DeserializeOwned>(name: &str) {
    match serde_json::from_value::<T>(json!({ "inconnu": true })) {
        Ok(_) => panic!("{name} reads an unknown field"),
        Err(e) => assert!(
            e.to_string().starts_with("unknown field `inconnu`"),
            "{name}: {e}"
        ),
    }
}

#[test]
fn every_request_body_of_the_contract_refuses_unknown_fields() {
    macro_rules! refuse {
        ($($body:ty),* $(,)?) => { $(refuses_unknown::<$body>(stringify!($body));)* };
    }
    crate::contract::request_bodies!(refuse);
}

#[test]
fn administration_bodies_read_what_the_web_client_sends() {
    let unit = "8d5ba3d6-0c4c-4b2c-9c51-2f8e6a3c1b7e";
    strict::<NewAccount>(
        json!({ "email": "a@neo-maintenance.example", "displayName": "Anne Martin" }),
        json!({
            "email": "a@neo-maintenance.example",
            "displayName": "Anne Martin",
            "jobTitle": "Technicienne",
            "orgUnitId": unit,
            "clearance": "confidential",
        }),
    );
    strict::<AccountUpdate>(
        json!({}),
        json!({
            "displayName": "Anne Martin",
            "jobTitle": null,
            "orgUnitId": null,
            "clearance": "internal",
            "status": "disabled",
            "attributes": { "department": "technique" },
        }),
    );
    strict::<RoleGrant>(
        json!({ "roleId": unit, "scope": { "type": "tenant" } }),
        json!({
            "roleId": unit,
            "scope": { "type": "org_unit", "orgUnitId": unit, "includeDescendants": true },
            "expiresAt": "2030-01-01T00:00:00Z",
        }),
    );
    strict::<NewOrgUnit>(
        json!({ "parentId": unit, "name": "Équipe Est", "type": "team" }),
        json!({ "parentId": unit, "name": "Équipe Est", "type": "team" }),
    );
    strict::<OrgUnitUpdate>(
        json!({}),
        json!({ "name": "Équipe Est", "type": "team", "parentId": unit, "leadUserId": null }),
    );
    strict::<NewRole>(
        json!({ "name": "Planificateur", "permissions": [] }),
        json!({ "name": "Planificateur", "description": "Planning", "permissions": ["planning:read"] }),
    );
    strict::<RoleUpdate>(
        json!({}),
        json!({ "name": "Planificateur", "description": "", "permissions": [] }),
    );
    let condition = json!({
        "op": "eq",
        "left": { "attr": "subject.attributes.contractType" },
        "right": { "value": "interim" },
    });
    strict::<NewPolicy>(
        json!({ "description": "Intérimaires", "permissions": ["intervention:read"], "condition": condition }),
        json!({
            "description": "Intérimaires",
            "permissions": ["intervention:read"],
            "condition": condition,
            "enabled": false,
        }),
    );
    strict::<PolicyUpdate>(
        json!({}),
        json!({ "description": "Intérimaires", "enabled": true }),
    );
    strict::<NewConnector>(
        json!({ "name": "Documents", "prefix": "docs", "url": "https://mcp.example/mcp" }),
        json!({
            "name": "Documents",
            "prefix": "docs",
            "url": "https://mcp.example/mcp",
            "token": "jeton",
            "enabled": false,
        }),
    );
    strict::<ConnectorUpdate>(
        json!({}),
        json!({
            "name": "Documents",
            "enabled": true,
            "url": "https://mcp.example/mcp",
            "token": null,
            "toolPolicies": { "search": { "enabled": true, "risk": "read", "permissions": [] } },
        }),
    );
    let operation = json!({
        "name": "get_ticket",
        "title": "Lire un ticket",
        "description": "Renvoie un ticket.",
        "method": "GET",
        "path": "/tickets/{id}",
    });
    strict::<NewRestConnector>(
        json!({ "name": "CRM", "prefix": "crm", "url": "https://api.example/v1", "operations": [operation] }),
        json!({
            "name": "CRM",
            "prefix": "crm",
            "url": "https://api.example/v1",
            "tokenHeader": "X-Api-Key",
            "token": "cle",
            "enabled": false,
            "operations": [{
                "name": "get_ticket",
                "title": "Lire un ticket",
                "description": "Renvoie un ticket.",
                "method": "GET",
                "path": "/tickets/{id}",
                "parameters": [
                    { "name": "id", "in": "path", "type": "string", "required": true, "description": "Son numéro." },
                ],
            }],
        }),
    );
    strict::<ConnectorUpdate>(
        json!({ "tokenHeader": null }),
        json!({ "tokenHeader": "X-Api-Key", "operations": [operation] }),
    );
    strict::<LlmChoice>(
        json!({ "provider": "anthropic" }),
        json!({ "provider": "openai_compatible", "model": "m", "baseUrl": "https://llm.example/v1", "apiKey": "cle" }),
    );
}

/// A tool policy and a role's scope refuse unknown fields too.
#[test]
fn nested_administration_values_refuse_unknown_fields() {
    let policy = json!({ "enabled": true, "inconnu": 1 });
    assert!(serde_json::from_value::<ToolPolicyChange>(policy).is_err());
    // A REST API's operation and its parameters as well (ADR-112).
    let parameter = json!({ "name": "id", "in": "path", "type": "string" });
    let operation = json!({
        "name": "a", "title": "A", "description": "A", "method": "GET", "path": "/a",
        "parameters": [parameter],
    });
    assert!(serde_json::from_value::<RestOperation>(operation.clone()).is_ok());
    let mut unknown = operation.clone();
    unknown["inconnu"] = json!(1);
    assert!(serde_json::from_value::<RestOperation>(unknown).is_err());
    let mut unknown = operation;
    unknown["parameters"][0]["inconnu"] = json!(1);
    assert!(serde_json::from_value::<RestOperation>(unknown).is_err());
    let unit = "8d5ba3d6-0c4c-4b2c-9c51-2f8e6a3c1b7e";
    // Every variant: a unit's fields on the whole company or on one's own
    // data would otherwise be ignored without a word.
    for scope in [
        json!({ "type": "org_unit", "orgUnitId": unit, "includeDescendants": true, "inconnu": 1 }),
        json!({ "type": "tenant", "inconnu": 1 }),
        json!({ "type": "tenant", "orgUnitId": unit, "includeDescendants": false }),
        json!({ "type": "self", "orgUnitId": unit, "includeDescendants": true }),
    ] {
        assert!(
            serde_json::from_value::<AssignmentScope>(scope.clone()).is_err(),
            "{scope}"
        );
    }
    // And they are answered as before.
    for (scope, written) in [
        (AssignmentScope::Tenant {}, json!({ "type": "tenant" })),
        (AssignmentScope::Own {}, json!({ "type": "self" })),
    ] {
        assert_eq!(serde_json::to_value(&scope).unwrap(), written);
        assert_eq!(
            serde_json::from_value::<AssignmentScope>(written).unwrap(),
            scope
        );
    }
}
