//! Accounts created by the administration tests: a newcomer invited by Paul
//! (technical administrator), the same holding a temporary password or once
//! it chose its own, one holding a single custom role, and an administrator
//! limited to one team, as if the role had been granted before the
//! company-wide rule existed.

use axum::http::StatusCode;
use mimorya_core::authz::AssignmentScope;
use mimorya_core::demo::{role, tenant, unit};
use mimorya_db::repositories::role_assignments::{NewRoleAssignment, create_role_assignment};
use serde_json::{Value, json};
use uuid::Uuid;

use super::mail::token_of;
use super::{Client, TestApi, TestResponse};

/// The password an account chooses to replace its temporary one.
pub const OWN_PASSWORD: &str = "phrase-de-passe-choisie";

pub struct NewAccount {
    pub id: Uuid,
    pub email: String,
    pub password: String,
}

/// The password an invited account chooses through its link.
pub const INVITED_PASSWORD: &str = "mot-de-passe-de-bienvenue";

/// Creates an account in the north team as `admin`; returns it with the
/// response's invitation (no e-mail relay: the link is in it).
pub async fn invite_account(admin: &Client<'_>, login: &str) -> (NewAccount, Value) {
    let email = format!("{login}@neo-maintenance.example");
    let created = admin
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
    let account = NewAccount {
        id: body["id"].as_str().unwrap().parse().unwrap(),
        email,
        password: String::new(),
    };
    (account, body["invitation"].clone())
}

/// Accepts an invitation link with `password`; returns the response.
pub async fn accept(api: &TestApi, link: &str, password: &str) -> TestResponse {
    api.anonymous()
        .post(
            "/auth/invitation/accept",
            json!({ "token": token_of(link), "password": password }),
        )
        .await
}

/// Creates an account in the north team as `admin`, activated through its
/// invitation, then reset by `admin` to a temporary password: the state most
/// administration tests start from.
pub async fn create_account(admin: &Client<'_>, login: &str) -> NewAccount {
    let (mut account, invitation) = invite_account(admin, login).await;
    let accepted = accept(
        admin.api(),
        invitation["link"].as_str().unwrap(),
        INVITED_PASSWORD,
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{}", accepted.text());
    let reset = admin
        .post(&format!("/admin/users/{}/password", account.id), json!({}))
        .await;
    assert_eq!(reset.status, StatusCode::OK, "{}", reset.text());
    account.password = reset.json()["temporaryPassword"]
        .as_str()
        .unwrap()
        .to_string();
    account
}

/// Signs the account in and replaces its temporary password, so that it can
/// use the application; returns that session.
pub async fn activate<'a>(api: &'a TestApi, account: &mut NewAccount) -> Client<'a> {
    let session = api.sign_in_with(&account.email, &account.password).await;
    let changed = session
        .post(
            "/account/password",
            json!({ "currentPassword": account.password, "newPassword": OWN_PASSWORD }),
        )
        .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.text());
    account.password = OWN_PASSWORD.to_string();
    session
}

/// A new account holding only this custom role, company-wide.
pub async fn holder_of<'a>(
    api: &'a TestApi,
    paul: &Client<'_>,
    login: &str,
    permissions: &[&str],
) -> Client<'a> {
    let role = paul
        .post(
            "/admin/roles",
            json!({ "name": format!("Rôle {login}"), "description": "Rôle de test", "permissions": permissions }),
        )
        .await;
    assert_eq!(role.status, StatusCode::CREATED, "{}", role.text());
    let mut account = create_account(paul, login).await;
    let granted = paul
        .post(
            &format!("/admin/users/{}/assignments", account.id),
            json!({ "roleId": role.json()["id"], "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(granted.status, StatusCode::CREATED, "{}", granted.text());
    activate(api, &mut account).await
}

/// An account holding the technical administrator role on the north team
/// only, written directly in the database (the API refuses such a grant).
pub async fn create_partial_admin(api: &TestApi, admin: &Client<'_>, login: &str) -> NewAccount {
    let account = create_account(admin, login).await;
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    create_role_assignment(
        &mut tx,
        NewRoleAssignment {
            tenant_id: tenant::ID,
            user_id: account.id,
            role_id: role::TENANT_ADMIN,
            scope: AssignmentScope::subtree(unit::TEAM_NORTH),
            granted_by_user_id: None,
            expires_at: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    account
}

pub fn permissions(me: &Value) -> Vec<String> {
    me["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap().to_string())
        .collect()
}

/// The directory entry of an account, as an administrator sees it.
pub async fn directory_entry(admin: &Client<'_>, user_id: Uuid) -> Value {
    admin.get("/admin/users").await.json()["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["id"] == json!(user_id))
        .expect("account listed")
        .clone()
}

/// Identifier of the account's assignment of `role_id`.
pub async fn assignment_of(admin: &Client<'_>, user_id: Uuid, role_id: Uuid) -> String {
    directory_entry(admin, user_id).await["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["roleId"] == json!(role_id))
        .expect("assignment listed")["id"]
        .as_str()
        .unwrap()
        .to_string()
}
