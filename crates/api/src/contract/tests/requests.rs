//! Request bodies are strict: a body the web client may send (with only its
//! required fields, or with every field) is read, and the same body with an
//! unknown field is refused, so that a misnamed field cannot be ignored.

use serde::de::DeserializeOwned;
use serde_json::{Value, json};

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
