//! Authentication, anti-CSRF protection and access to administration screens.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mimorya_core::demo::user;
use serde_json::json;
use support::{PASSWORD, TestApi, email};

fn login_request(headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::post("/auth/login").header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let payload = json!({ "email": email(user::MARC), "password": PASSWORD });
    builder.body(Body::from(payload.to_string())).unwrap()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn rejects_state_changing_requests_without_the_header_or_from_another_origin() {
    let api = TestApi::start().await;
    let without_header = api.send(login_request(&[])).await;
    assert_eq!(without_header.status, StatusCode::FORBIDDEN);
    let other_origin = api
        .send(login_request(&[
            ("x-mimorya-request", "1"),
            ("origin", "https://evil.example"),
        ]))
        .await;
    assert_eq!(other_origin.status, StatusCode::FORBIDDEN);
    assert_eq!(other_origin.error_message(), "Origine non autorisée");
}

/// The SAML reception alone is posted without the header and from another
/// site (ADR-068): no other request, even on a path that merely resembles
/// it, goes without either.
#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn only_the_saml_reception_goes_without_the_header() {
    let api = TestApi::start().await;
    let from_a_provider = |path: &str| {
        Request::post(path)
            .header("content-type", "application/x-www-form-urlencoded")
            .header("origin", "https://idp.saml.example")
            .body(Body::from("SAMLResponse="))
            .unwrap()
    };
    // Reached: it judges the response itself (without an attempt, back to
    // the sign-in page).
    let reception = api.send(from_a_provider("/auth/saml/acs")).await;
    assert_eq!(reception.status, StatusCode::SEE_OTHER);
    for path in [
        "/auth/saml/acs/",
        "/auth/saml/ACS",
        "/auth/saml/acs2",
        "/auth/saml//acs",
        "/auth/saml/%61cs",
        "/auth/saml/00000000-0000-4000-8000-000000000001/metadata",
        "/auth/sso/callback",
        "/auth/login",
    ] {
        let refused = api.send(from_a_provider(path)).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            refused.error_message(),
            "En-tête de requête manquant",
            "{path}"
        );
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn answers_identically_for_unknown_accounts_and_wrong_passwords() {
    let api = TestApi::start().await;
    let wrong = api.try_login(&email(user::MARC), "incorrect").await;
    let unknown = api.try_login("personne@inconnu.example", "incorrect").await;
    assert_eq!(wrong.status, StatusCode::UNAUTHORIZED);
    assert_eq!(unknown.status, StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.json(), unknown.json());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn rejects_malformed_logins_with_a_validation_error() {
    let api = TestApi::start().await;
    let response = api.try_login("pas-une-adresse", PASSWORD).await;
    assert_eq!(response.status, StatusCode::BAD_REQUEST);
    assert_eq!(response.json()["error"]["code"], "validation");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn exposes_effective_permissions_computed_server_side() {
    let api = TestApi::start().await;
    let marc = api.sign_in(user::MARC).await;
    let me = marc.get("/auth/me").await.json();
    let permissions: Vec<&str> = me["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    assert!(permissions.contains(&"intervention:read"));
    assert!(!permissions.contains(&"user:manage"));
    assert!(me["grants"].as_array().unwrap().contains(&json!(
        "Responsable d’équipe — Service technique et sous-unités"
    )));
    assert_eq!(me["tenant"]["demo"], true);

    let anonymous = api
        .send(Request::get("/auth/me").body(Body::empty()).unwrap())
        .await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let forged = api.as_user("jeton-inventé").get("/auth/me").await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn revokes_the_session_on_logout() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let logout = lucas.post("/auth/logout", json!({})).await;
    assert_eq!(logout.status, StatusCode::OK);
    assert!(
        logout.headers["set-cookie"]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(lucas.get("/auth/me").await.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn reserves_administration_screens_to_the_matching_permissions() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    assert_eq!(paul.get("/admin/audit").await.status, StatusCode::OK);
    assert_eq!(marc.get("/admin/audit").await.status, StatusCode::FORBIDDEN);
    // Reading skills is a member right; the administration screen is not.
    assert_eq!(paul.get("/admin/skills").await.status, StatusCode::OK);
    assert_eq!(
        marc.get("/admin/skills").await.status,
        StatusCode::FORBIDDEN
    );

    let users = paul.get("/admin/users").await.json();
    let users = users["users"].as_array().unwrap();
    assert_eq!(users.len(), 10);
    assert!(users[0]["grants"].is_array());
    // Without role management, the directory hides emails and grants.
    let directory = marc.get("/admin/users").await.json();
    assert!(directory["users"][0].get("email").is_none());

    let audit = paul.get("/admin/audit?limit=5").await.json();
    assert!(audit["entries"].as_array().unwrap().len() <= 5);
    assert_eq!(
        paul.get("/admin/audit?limit=0").await.status,
        StatusCode::BAD_REQUEST
    );
    for screen in [
        "/admin/organization",
        "/admin/roles",
        "/admin/usage",
        "/admin/connectors",
    ] {
        assert_eq!(paul.get(screen).await.status, StatusCode::OK, "{screen}");
    }
}
