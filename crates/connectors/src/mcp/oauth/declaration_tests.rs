//! The declaration as kept: who authorizes and whether identities are
//! verified written only when they differ from what older processes know,
//! and the client written in visible ASCII.

use super::declaration::*;
use super::server_metadata::ClientAuth;

fn declaration(holder: OAuthHolder) -> OAuthDeclaration {
    OAuthDeclaration {
        issuer: "https://as.example".into(),
        resource: "https://mcp.example/mcp".into(),
        authorization_endpoint: "https://as.example/authorize".into(),
        token_endpoint: "https://as.example/token".into(),
        names_issuer_on_return: false,
        client_id: "mimorya".into(),
        client_auth: ClientAuth::None,
        scopes: Vec::new(),
        revocation_endpoint: None,
        registered: None,
        holder,
        verifies_identity: false,
    }
}

#[test]
fn a_declaration_kept_before_the_holder_is_the_company_s() {
    let kept = serde_json::json!({
        "issuer": "https://as.example",
        "resource": "https://mcp.example/mcp",
        "authorizationEndpoint": "https://as.example/authorize",
        "tokenEndpoint": "https://as.example/token",
        "namesIssuerOnReturn": false,
        "clientId": "mimorya",
        "clientAuth": "none",
        "scopes": [],
    });
    let read: OAuthDeclaration = serde_json::from_value(kept.clone()).expect("read");
    assert_eq!(read, declaration(OAuthHolder::Company));
    // Written as before: a process that knows only the company's reads it.
    let written = serde_json::to_value(&read).expect("written");
    assert_eq!(written, kept);
}

#[test]
fn each_person_s_holder_is_written_and_read_back() {
    let person = declaration(OAuthHolder::Person);
    let written = serde_json::to_value(&person).expect("written");
    assert_eq!(written["holder"], "person");
    let read: OAuthDeclaration = serde_json::from_value(written).expect("read");
    assert_eq!(read, person);
    let unknown = serde_json::json!({ "holder": "team" });
    let mut kept = serde_json::to_value(&person).expect("written");
    kept["holder"] = unknown["holder"].clone();
    assert!(serde_json::from_value::<OAuthDeclaration>(kept).is_err());
}

#[test]
fn a_verification_of_identities_is_written_only_when_asked() {
    let each = declaration(OAuthHolder::Person);
    let written = serde_json::to_value(&each).expect("written");
    assert!(written.get("verifiesIdentity").is_none(), "{written}");
    let verifying = OAuthDeclaration {
        verifies_identity: true,
        ..each
    };
    let written = serde_json::to_value(&verifying).expect("written");
    assert_eq!(written["verifiesIdentity"], true);
    let read: OAuthDeclaration = serde_json::from_value(written).expect("read");
    assert_eq!(read, verifying);
}

#[test]
fn a_client_is_written_in_visible_ascii() {
    for good in [
        "mimorya",
        "0oa1b2c3d4",
        "https://mimorya.example/client",
        "a b",
    ] {
        assert_eq!(client_id_problem(good), None, "{good}");
    }
    for bad in [
        "",
        " mimorya",
        "mimorya ",
        "mimoryà",
        "mi\nmorya",
        "mi\u{200b}morya",
    ] {
        assert!(client_id_problem(bad).is_some(), "{bad:?}");
    }
    assert!(client_id_problem(&"a".repeat(MAX_CLIENT_ID_CHARS + 1)).is_some());
    assert_eq!(
        client_secret_problem(&"s".repeat(MAX_CLIENT_SECRET_CHARS)),
        None
    );
    let refused = client_secret_problem("secret\u{202e}").expect("refused");
    assert!(!refused.contains("secret\u{202e}"), "{refused}");
}
