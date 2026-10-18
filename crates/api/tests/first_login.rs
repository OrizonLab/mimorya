//! An account holding a temporary password (created or reset by an
//! administrator) reaches nothing but its own identity, the logout and the
//! password change until it has chosen its own password.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, user};
use serde_json::json;
use support::accounts::{OWN_PASSWORD, activate, create_account};
use support::{Client, TestApi};
use uuid::Uuid;

async fn assert_held_back(session: &Client<'_>) {
    let me = session.get("/auth/me").await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.json()["user"]["mustChangePassword"], true);
    for url in [
        "/conversations",
        "/approvals",
        "/memory",
        "/admin/organization",
    ] {
        let refused = session.get(url).await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{url}");
        assert_eq!(
            refused.json()["error"]["details"]["reason"],
            "password_change_required",
            "{url}"
        );
    }
    let write = session
        .post("/conversations", json!({ "title": "Avant le changement" }))
        .await;
    assert_eq!(write.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_new_account_chooses_its_password_before_anything_else() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "premiere.connexion").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    assert_held_back(&session).await;

    let changed = session
        .post(
            "/account/password",
            json!({ "currentPassword": account.password, "newPassword": OWN_PASSWORD }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK);
    // The same session goes on, now unrestricted.
    let me = session.get("/auth/me").await.json();
    assert_eq!(me["user"]["mustChangePassword"], false);
    assert_eq!(session.get("/conversations").await.status, StatusCode::OK);
    // Demo accounts chose theirs.
    let marc = api.sign_in(user::MARC).await;
    assert_eq!(
        marc.get("/auth/me").await.json()["user"]["mustChangePassword"],
        false
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_reset_requires_a_new_choice_and_logging_out_stays_possible() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let mut account = create_account(&paul, "nouveau.choix").await;
    activate(&api, &mut account).await;

    let reset = paul
        .post(&format!("/admin/users/{}/password", account.id), json!({}))
        .await;
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    let session = api.sign_in_with(&account.email, &temporary).await;
    assert_held_back(&session).await;
    // Routes needing no identity stay open with that cookie: the login page
    // works, for this account as for another one.
    assert_eq!(session.get("/health").await.status, StatusCode::OK);
    for (email, password) in [
        (account.email.clone(), temporary.clone()),
        (support::email(user::MARC), support::PASSWORD.to_string()),
    ] {
        let login = session
            .post(
                "/auth/login",
                json!({ "email": email, "password": password }),
            )
            .await;
        assert_eq!(login.status, StatusCode::OK, "{email}: {}", login.text());
    }
    assert_eq!(
        session.post("/auth/logout", json!({})).await.status,
        StatusCode::OK
    );
    assert_eq!(
        session.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
}

/// Every route needing an identity (placeholder identifiers): keep it in step
/// with the router when a route is added.
const AUTHENTICATED_ROUTES: &[(&str, &str)] = &[
    ("GET", "/conversations"),
    ("POST", "/conversations"),
    ("GET", "/conversations/{id}"),
    ("PATCH", "/conversations/{id}"),
    ("POST", "/conversations/{id}/messages"),
    ("GET", "/runs"),
    ("GET", "/runs/{id}"),
    ("GET", "/runs/{id}/events"),
    ("GET", "/approvals"),
    ("POST", "/approvals/{id}/decision"),
    ("GET", "/memory"),
    ("POST", "/memory"),
    ("DELETE", "/memory/{id}"),
    ("POST", "/memory/{id}/review"),
    ("GET", "/admin/organization"),
    ("POST", "/admin/org-units"),
    ("PATCH", "/admin/org-units/{id}"),
    ("DELETE", "/admin/org-units/{id}"),
    ("GET", "/admin/users"),
    ("POST", "/admin/users"),
    ("PATCH", "/admin/users/{id}"),
    ("POST", "/admin/users/{id}/password"),
    ("POST", "/admin/users/{id}/assignments"),
    ("DELETE", "/admin/assignments/{id}"),
    ("GET", "/admin/roles"),
    ("POST", "/admin/roles"),
    ("PATCH", "/admin/roles/{id}"),
    ("DELETE", "/admin/roles/{id}"),
    ("GET", "/admin/policies"),
    ("POST", "/admin/policies"),
    ("PATCH", "/admin/policies/{id}"),
    ("DELETE", "/admin/policies/{id}"),
    ("GET", "/admin/connectors"),
    ("GET", "/admin/skills"),
    ("GET", "/admin/audit"),
    ("GET", "/admin/usage"),
];

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn every_authenticated_route_holds_a_temporary_password_back() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "administrateur.provisoire").await;
    // A company-wide administrator: only the temporary password holds it back.
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role::TENANT_ADMIN, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED);
    let session = api.sign_in_with(&account.email, &account.password).await;
    for (method, path) in AUTHENTICATED_ROUTES {
        let url = path.replace("{id}", &Uuid::new_v4().to_string());
        let response = match *method {
            "GET" => session.get(&url).await,
            "POST" => session.post(&url, json!({})).await,
            "PATCH" => session.patch(&url, json!({})).await,
            _ => session.delete(&url).await,
        };
        assert_eq!(response.status, StatusCode::FORBIDDEN, "{method} {path}");
        assert_eq!(
            response.json()["error"]["details"]["reason"],
            "password_change_required",
            "{method} {path}"
        );
    }
}
