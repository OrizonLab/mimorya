//! An account holding a temporary password (created or reset by an
//! administrator) reaches nothing but its own identity, the logout and the
//! password change until it has chosen its own password.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::user;
use serde_json::json;
use support::accounts::{OWN_PASSWORD, activate, create_account};
use support::{Client, TestApi};

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
    assert_eq!(
        session.post("/auth/logout", json!({})).await.status,
        StatusCode::OK
    );
    assert_eq!(
        session.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
}
