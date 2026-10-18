//! Account administration: creation with a temporary password, status and
//! password changes, role assignments, and the limits of the administrator's
//! own power (no self-administration, company-wide scope only).

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_db::repositories::role_assignments::{NewRoleAssignment, create_role_assignment};
use serde_json::{Value, json};
use support::{Client, TestApi};
use uuid::Uuid;

/// Creates an account as Paul (technical administrator); returns its id,
/// email and temporary password.
async fn create_account(paul: &Client<'_>, login: &str) -> (Uuid, String, String) {
    let email = format!("{login}@neo-maintenance.example");
    let created = paul
        .post(
            "/admin/users",
            json!({
                "email": email,
                "displayName": "Nouveau Collègue",
                "jobTitle": "Technicien",
                "orgUnitId": unit::TEAM_NORTH,
            }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let body = created.json();
    let id = body["id"].as_str().unwrap().parse().unwrap();
    (
        id,
        email,
        body["temporaryPassword"].as_str().unwrap().to_string(),
    )
}

fn permissions(me: &Value) -> Vec<String> {
    me["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect()
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn creates_accounts_that_sign_in_with_a_temporary_password() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, email, password) = create_account(&paul, "nouveau.collegue").await;

    let login = api.try_login(&email, &password).await;
    assert_eq!(login.status, StatusCode::OK);
    let newcomer = api.sign_in_with(&email, &password).await;
    let me = newcomer.get("/auth/me").await.json();
    assert!(permissions(&me).contains(&"agent:use".to_string()));
    assert_eq!(me["user"]["orgUnit"]["id"], json!(unit::TEAM_NORTH));

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
    let listed = directory["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"] == json!(id))
        .unwrap()
        .clone();
    assert_eq!(listed["email"], json!(email));
    assert_eq!(listed["assignments"][0]["roleName"], "Collaborateur");

    // The newcomer replaces the temporary password.
    let wrong = newcomer
        .post(
            "/account/password",
            json!({ "currentPassword": "incorrect", "newPassword": "une-phrase-de-passe-solide" }),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::FORBIDDEN);
    let changed = newcomer
        .post(
            "/account/password",
            json!({ "currentPassword": password, "newPassword": "une-phrase-de-passe-solide" }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(
        api.try_login(&email, &password).await.status,
        StatusCode::UNAUTHORIZED
    );
    let renewed = api.try_login(&email, "une-phrase-de-passe-solide").await;
    assert_eq!(renewed.status, StatusCode::OK);
    assert_eq!(newcomer.get("/auth/me").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn disables_accounts_at_once_and_resets_passwords() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (id, email, password) = create_account(&paul, "compte.a.desactiver").await;
    let account = api.sign_in_with(&email, &password).await;
    let url = format!("/admin/users/{id}");

    assert_eq!(
        paul.patch(&url, json!({ "status": "disabled" }))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        account.get("/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.try_login(&email, &password).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        paul.patch(&url, json!({ "status": "active", "jobTitle": null }))
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        api.try_login(&email, &password).await.status,
        StatusCode::OK
    );

    let reset = paul.post(&format!("{url}/password"), json!({})).await;
    assert_eq!(reset.status, StatusCode::OK);
    let temporary = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        api.try_login(&email, &password).await.status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        api.try_login(&email, &temporary).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refuses_self_administration() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let own = format!("/admin/users/{}", user::PAUL);
    let disable = paul.patch(&own, json!({ "status": "disabled" })).await;
    assert_eq!(disable.status, StatusCode::FORBIDDEN);
    let grant = paul
        .post(
            &format!("{own}/assignments"),
            json!({ "roleId": role::DIRECTOR, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(grant.status, StatusCode::FORBIDDEN);
    assert_eq!(
        paul.post(&format!("{own}/password"), json!({}))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn grants_roles_on_their_scope_and_revokes_them() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let (id, email, password) = create_account(&paul, "futur.chef").await;
    let account = api.sign_in_with(&email, &password).await;
    let grants = format!("/admin/users/{id}/assignments");

    let team_lead = json!({
        "roleId": role::TEAM_LEAD,
        "scope": { "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true },
    });
    assert_eq!(
        marc.post(&grants, team_lead.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    let granted = paul.post(&grants, team_lead).await;
    assert_eq!(granted.status, StatusCode::CREATED);
    let assignment = granted.json()["id"].as_str().unwrap().to_string();
    let me = account.get("/auth/me").await.json();
    assert!(permissions(&me).contains(&"planning:apply".to_string()));

    // Administration roles cover the whole company or nothing.
    let admin_on_unit = json!({
        "roleId": role::TENANT_ADMIN,
        "scope": { "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true },
    });
    assert_eq!(
        paul.post(&grants, admin_on_unit).await.status,
        StatusCode::BAD_REQUEST
    );
    let expired = json!({
        "roleId": role::DIRECTOR,
        "scope": { "type": "tenant" },
        "expiresAt": "2020-01-01T00:00:00Z",
    });
    assert_eq!(
        paul.post(&grants, expired).await.status,
        StatusCode::BAD_REQUEST
    );

    let revoke = format!("/admin/assignments/{assignment}");
    assert_eq!(marc.delete(&revoke).await.status, StatusCode::FORBIDDEN);
    assert_eq!(paul.delete(&revoke).await.status, StatusCode::OK);
    let me = account.get("/auth/me").await.json();
    assert!(!permissions(&me).contains(&"planning:apply".to_string()));
    assert_eq!(paul.delete(&revoke).await.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn honours_administration_permissions_on_the_whole_company_only() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (id, email, password) = create_account(&paul, "admin.partiel").await;
    // As if granted on one team before the rule existed (or by hand).
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    create_role_assignment(
        &mut tx,
        NewRoleAssignment {
            tenant_id: tenant::ID,
            user_id: id,
            role_id: role::TENANT_ADMIN,
            scope: AssignmentScope::subtree(unit::TEAM_NORTH),
            granted_by_user_id: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let partial = api.sign_in_with(&email, &password).await;
    for screen in [
        "/admin/audit",
        "/admin/roles",
        "/admin/connectors",
        "/admin/skills",
    ] {
        assert_eq!(
            partial.get(screen).await.status,
            StatusCode::FORBIDDEN,
            "{screen}"
        );
    }
    let directory = partial.get("/admin/users").await.json();
    assert_eq!(
        directory["canManage"],
        json!({ "users": false, "roles": false })
    );
    assert!(directory["users"][0].get("email").is_none());
    let create = partial
        .post(
            "/admin/users",
            json!({ "email": "y@neo-maintenance.example", "displayName": "Yves Essai" }),
        )
        .await;
    assert_eq!(create.status, StatusCode::FORBIDDEN);
}
