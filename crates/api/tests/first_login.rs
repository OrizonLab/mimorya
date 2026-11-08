//! An account holding a temporary password (created or reset by an
//! administrator) reaches nothing but its own identity, the logout and the
//! password change until it has chosen its own password.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use axum::http::StatusCode;
use mimorya_core::demo::{role, user};
use regex::Regex;
use serde_json::json;
use support::accounts::{OWN_PASSWORD, activate, create_account};
use support::routes::{AUTHENTICATED_ROUTES, OTHER_ROUTES};
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
            "PUT" => session.put(&url, json!({})).await,
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

/// The `(method, path)` pairs a routes file declares, read from the arguments
/// of each `.route(…)` call.
fn declared_routes(source: &str) -> Vec<(String, String)> {
    let path = Regex::new(r#"^\s*"([^"]+)""#).unwrap();
    let method = Regex::new(r"\b(get|post|patch|put|delete)\(").unwrap();
    let mut routes = Vec::new();
    for call in source.split(".route(").skip(1) {
        // The call's own arguments, up to its closing parenthesis.
        let mut depth = 1;
        let end = call
            .char_indices()
            .find(|(_, c)| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth == 0
            })
            .map_or(call.len(), |(i, _)| i);
        let arguments = &call[..end];
        let declared = path
            .captures(arguments)
            .unwrap_or_else(|| panic!("unreadable route: .route({arguments})"));
        let before = routes.len();
        for found in method.captures_iter(arguments) {
            routes.push((found[1].to_uppercase(), declared[1].to_string()));
        }
        // `any(…)`, `on(…)` or a service: extend this test before using one.
        assert!(
            routes.len() > before,
            "no known method for {}",
            &declared[1]
        );
    }
    routes
}

#[test]
fn the_route_list_follows_the_router() {
    let mut declared = BTreeSet::new();
    let mut directories = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes")];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                directories.push(path);
            } else {
                declared.extend(declared_routes(&fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let listed: BTreeSet<(String, String)> = AUTHENTICATED_ROUTES
        .iter()
        .chain(OTHER_ROUTES)
        .map(|(method, path)| ((*method).to_string(), (*path).to_string()))
        .collect();
    assert!(!declared.is_empty());
    assert_eq!(
        declared, listed,
        "AUTHENTICATED_ROUTES or OTHER_ROUTES no longer match the router"
    );
}
