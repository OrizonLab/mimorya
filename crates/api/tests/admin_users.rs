//! Account administration: creation with a temporary password, profile,
//! status and password changes, and what an administrator cannot reach
//! (unknown units, accounts of another company).

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{unit, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::json;
use support::TestApi;
use support::accounts::{create_account, directory_entry, permissions};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn creates_accounts_that_sign_in_with_a_temporary_password() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let created = paul
        .post(
            "/admin/users",
            json!({ "email": "Nouveau.Collegue@neo-maintenance.example", "displayName": "Nouveau Collègue" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    // The temporary password is shown once and never kept by a cache.
    assert_eq!(created.headers["cache-control"], "no-store");
    let body = created.json();
    let email = "nouveau.collegue@neo-maintenance.example";
    let password = body["temporaryPassword"].as_str().unwrap();

    let newcomer = api.sign_in_with(email, password).await;
    let me = newcomer.get("/auth/me").await.json();
    assert!(permissions(&me).contains(&"agent:use".to_string()));

    let duplicate = paul
        .post(
            "/admin/users",
            json!({ "email": email, "displayName": "Doublon" }),
        )
        .await;
    assert_eq!(duplicate.status, StatusCode::CONFLICT);
    let refused = marc
        .post(
            "/admin/users",
            json!({ "email": "x@neo-maintenance.example", "displayName": "Xavier Essai" }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);

    let directory = paul.get("/admin/users").await.json();
    assert_eq!(
        directory["canManage"],
        json!({ "users": true, "roles": true })
    );
    let listed = directory_entry(&paul, body["id"].as_str().unwrap().parse().unwrap()).await;
    assert_eq!(listed["email"], json!(email));
    assert_eq!(listed["assignments"][0]["roleName"], "Collaborateur");
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn changes_profiles_clearance_and_primary_unit() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "profil.modifie").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let url = format!("/admin/users/{}", account.id);
    assert_eq!(
        directory_entry(&paul, account.id).await["clearance"],
        "internal"
    );

    let changed = paul
        .patch(
            &url,
            json!({
                "displayName": "Camille Durand",
                "jobTitle": "Cheffe d’équipe",
                "clearance": "confidential",
                "orgUnitId": unit::TEAM_SOUTH,
            }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    let me = session.get("/auth/me").await.json();
    assert_eq!(me["user"]["displayName"], "Camille Durand");
    assert_eq!(me["user"]["jobTitle"], "Cheffe d’équipe");
    assert_eq!(me["user"]["orgUnit"]["id"], json!(unit::TEAM_SOUTH));
    let listed = directory_entry(&paul, account.id).await;
    assert_eq!(listed["clearance"], "confidential");
    assert_eq!(listed["orgUnitId"], json!(unit::TEAM_SOUTH));

    let detached = paul
        .patch(&url, json!({ "orgUnitId": null, "jobTitle": null }))
        .await;
    assert_eq!(detached.status, StatusCode::OK);
    let me = session.get("/auth/me").await.json();
    assert!(me["user"]["orgUnit"].is_null());
    assert!(me["user"]["jobTitle"].is_null());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn disables_accounts_at_once() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "compte.a.desactiver").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let url = format!("/admin/users/{}", account.id);

    assert_eq!(
        paul.patch(&url, json!({ "status": "disabled" }))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        session.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        paul.patch(&url, json!({ "status": "active" })).await.status,
        StatusCode::OK
    );
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::OK
    );
    // Sessions ended by the disabling stay ended (a stolen cookie included).
    assert_eq!(
        session.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn resets_passwords_and_ends_open_sessions() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "mot.de.passe.oublie").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    assert_eq!(session.get("/auth/me").await.status, StatusCode::OK);

    let reset = paul
        .post(&format!("/admin/users/{}/password", account.id), json!({}))
        .await;
    assert_eq!(reset.status, StatusCode::OK);
    assert_eq!(reset.headers["cache-control"], "no-store");
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        session.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.try_login(&account.email, &account.password)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.try_login(&account.email, &temporary).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn rejects_unknown_units_and_accounts_of_other_companies() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let unknown_unit = uuid::Uuid::new_v4();
    let create = paul
        .post(
            "/admin/users",
            json!({
                "email": "sans.unite@neo-maintenance.example",
                "displayName": "Sans Unité",
                "orgUnitId": unknown_unit,
            }),
        )
        .await;
    assert_eq!(create.status, StatusCode::BAD_REQUEST);
    let account = create_account(&paul, "rattachement.inconnu").await;
    let moved = paul
        .patch(
            &format!("/admin/users/{}", account.id),
            json!({ "orgUnitId": unknown_unit }),
        )
        .await;
    assert_eq!(moved.status, StatusCode::BAD_REQUEST);

    // An account of another company does not exist from here.
    let foreign = format!("/admin/users/{OTHER_USER_ID}");
    assert_eq!(
        paul.patch(&foreign, json!({ "status": "disabled" }))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        paul.post(&format!("{foreign}/password"), json!({}))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}
