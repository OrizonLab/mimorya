//! The identity a person proves at an MCP server's authorization server
//! in the tests (ADR-175): a server each person authorizes, declared so
//! that it verifies who authorizes it; the person the simulated server
//! signs in; and why a return was refused.

use axum::http::StatusCode;
use mimorya_connectors::testing::{CLIENT_ID, CLIENT_SECRET, FakeAuthorizationServer, SignedIn};
use serde_json::{Value, json};
use uuid::Uuid;

use super::person_authorizations::entries_by;
use super::{Client, TestApi, TestResponse, email};

/// What the audit calls a person's own authorization.
pub const PERSON_AUTHORIZED_ACTION: &str = "connector.person_oauth_authorized";
/// Where a person refused for another's account is sent back.
pub const PERSON_OTHER_ACCOUNT: &str = "/account?serveur=autre-compte";

/// Signs `person` in at the authorization server, their address verified.
pub fn signed_in_as(authority: &FakeAuthorizationServer, person: Uuid) {
    let signed_in = SignedIn::verified(&email(person));
    authority.set(|settings| settings.identity = Some(signed_in));
}

/// The declaration `body` of the server, with the test client: its answer.
pub async fn declare(admin: &Client<'_>, id: &str, body: Value) -> TestResponse {
    let mut declaration = json!({ "clientId": CLIENT_ID, "clientSecret": CLIENT_SECRET });
    if let (Some(declaration), Some(body)) = (declaration.as_object_mut(), body.as_object()) {
        declaration.extend(body.clone());
    }
    admin
        .put(&format!("/admin/connectors/{id}/oauth"), declaration)
        .await
}

/// A server protected by `authority`, each person authorizing it, who they
/// are verified there: the server's address and the connector's id.
pub async fn declared_verifying(
    admin: &Client<'_>,
    authority: &FakeAuthorizationServer,
    prefix: &str,
) -> (String, String) {
    let (server, id) = super::connector_oauth::declared(admin, authority, prefix).await;
    let body = json!({ "perPerson": true, "verifyIdentity": true });
    let declared = declare(admin, &id, body).await;
    assert_eq!(declared.status, StatusCode::OK, "{}", declared.text());
    (server.url, id)
}

/// Why `person`'s last return for the connector was refused, as audited.
pub async fn refused_because(api: &TestApi, id: &str, person: Uuid) -> Option<String> {
    let entries = entries_by(api, Some(id), person, PERSON_AUTHORIZED_ACTION).await;
    entries
        .into_iter()
        .rev()
        .find(|(outcome, _)| outcome == "denied")
        .and_then(|(_, reason)| reason)
}
