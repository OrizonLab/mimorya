//! Custom roles: created from the permission catalog, their changes apply to
//! their holders at once; platform roles and the roles one holds cannot be
//! changed; administration roles stay company-wide; an assigned role is not
//! deleted.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, unit, user};
use serde_json::{Value, json};
use support::accounts::{activate, assignment_of, create_account, permissions};
use support::{Client, TestApi};
use uuid::Uuid;

async fn create_role(paul: &Client<'_>, name: &str, permissions: &[&str]) -> String {
    let created = paul
        .post(
            "/admin/roles",
            json!({ "name": name, "description": "Rôle de test", "permissions": permissions }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_string()
}

async fn listed(paul: &Client<'_>, id: &str) -> Option<Value> {
    paul.get("/admin/roles").await.json()["roles"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .cloned()
}

fn on_unit(role_id: &str) -> Value {
    json!({
        "roleId": role_id,
        "scope": { "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true },
    })
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn custom_roles_grant_their_permissions_and_follow_their_changes() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let id = create_role(
        &paul,
        "Chef de chantier",
        &["intervention:read", "planning:apply"],
    )
    .await;
    let role = listed(&paul, &id).await.unwrap();
    assert_eq!(role["isSystem"], false);
    assert_eq!(role["assignments"], 0);
    assert_eq!(role["key"], "custom_chef_de_chantier");

    let mut account = create_account(&paul, "chef.chantier").await;
    let session = activate(&api, &mut account).await;
    let grants = format!("/admin/users/{}/assignments", account.id);
    assert_eq!(
        paul.post(&grants, on_unit(&id)).await.status,
        StatusCode::CREATED
    );
    let me = session.get("/auth/me").await.json();
    assert!(permissions(&me).contains(&"planning:apply".to_string()));

    // A narrower role takes the permission away from its holders at once.
    let narrowed = paul
        .patch(
            &format!("/admin/roles/{id}"),
            json!({ "permissions": ["intervention:read"] }),
        )
        .await;
    assert_eq!(narrowed.status, StatusCode::OK, "{}", narrowed.text());
    let me = session.get("/auth/me").await.json();
    assert!(!permissions(&me).contains(&"planning:apply".to_string()));
    assert!(permissions(&me).contains(&"intervention:read".to_string()));

    let refused = marc
        .post(
            "/admin/roles",
            json!({ "name": "Rôle pirate", "permissions": ["intervention:read"] }),
        )
        .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn refuses_invalid_names_and_permissions() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    create_role(&paul, "Astreinte", &["intervention:read"]).await;
    let attempts = [
        (
            json!({ "name": "ASTREINTE", "permissions": ["intervention:read"] }),
            StatusCode::CONFLICT,
        ),
        (
            json!({ "name": "Sans droit", "permissions": [] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "name": "Inconnu", "permissions": ["intervention:delete"] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "name": "Joker", "permissions": ["*"] }),
            StatusCode::BAD_REQUEST,
        ),
        (
            json!({ "name": "— !", "permissions": ["intervention:read"] }),
            StatusCode::BAD_REQUEST,
        ),
    ];
    for (body, expected) in attempts {
        let response = paul.post("/admin/roles", body.clone()).await;
        assert_eq!(response.status, expected, "{body}: {}", response.text());
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn platform_roles_and_the_roles_one_holds_are_not_changed() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let admin_role = format!("/admin/roles/{}", role::TENANT_ADMIN);
    let widened = paul
        .patch(&admin_role, json!({ "permissions": ["intervention:read"] }))
        .await;
    assert_eq!(widened.status, StatusCode::FORBIDDEN);
    assert_eq!(
        paul.delete(&format!("/admin/roles/{}", role::MEMBER))
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // A second administrator holding a custom role cannot change it.
    let id = create_role(&paul, "Lecture interventions", &["intervention:read"]).await;
    let mut account = create_account(&paul, "second.administrateur").await;
    let grants = format!("/admin/users/{}/assignments", account.id);
    for granted in [
        json!({ "roleId": role::TENANT_ADMIN, "scope": { "type": "tenant" } }),
        json!({ "roleId": id, "scope": { "type": "tenant" } }),
    ] {
        assert_eq!(
            paul.post(&grants, granted).await.status,
            StatusCode::CREATED
        );
    }
    let second = activate(&api, &mut account).await;
    let own = format!("/admin/roles/{id}");
    let widened = second
        .patch(
            &own,
            json!({ "permissions": ["intervention:read", "planning:apply"] }),
        )
        .await;
    assert_eq!(widened.status, StatusCode::FORBIDDEN, "{}", widened.text());
    assert_eq!(second.delete(&own).await.status, StatusCode::FORBIDDEN);
    assert_eq!(
        paul.patch(
            &format!("/admin/roles/{}", Uuid::new_v4()),
            json!({ "name": "Fantôme" })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn administration_roles_stay_company_wide() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = create_role(&paul, "Responsable de site", &["intervention:read"]).await;
    let account = create_account(&paul, "responsable.site").await;
    let grants = format!("/admin/users/{}/assignments", account.id);
    let granted = paul.post(&grants, on_unit(&id)).await;
    assert_eq!(granted.status, StatusCode::CREATED);

    // Adding an administration permission would leave it granted on a unit.
    let url = format!("/admin/roles/{id}");
    let widened = json!({ "permissions": ["intervention:read", "user:manage"] });
    let refused = paul.patch(&url, widened.clone()).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.text());
    let assignment = granted.json()["id"].as_str().unwrap().to_string();
    paul.delete(&format!("/admin/assignments/{assignment}"))
        .await;
    assert_eq!(paul.patch(&url, widened).await.status, StatusCode::OK);
    // From now on it is only granted company-wide.
    assert_eq!(
        paul.post(&grants, on_unit(&id)).await.status,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn deletes_only_roles_nobody_holds() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let id = create_role(&paul, "Rôle provisoire", &["intervention:read"]).await;
    let account = create_account(&paul, "role.provisoire").await;
    let grants = format!("/admin/users/{}/assignments", account.id);
    assert_eq!(
        paul.post(&grants, on_unit(&id)).await.status,
        StatusCode::CREATED
    );
    let url = format!("/admin/roles/{id}");
    let refused = paul.delete(&url).await;
    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert!(refused.error_message().contains("1 habilitation"));

    let role_id: Uuid = id.parse().unwrap();
    let assignment = assignment_of(&paul, account.id, role_id).await;
    paul.delete(&format!("/admin/assignments/{assignment}"))
        .await;
    assert_eq!(paul.delete(&url).await.status, StatusCode::OK);
    assert!(listed(&paul, &id).await.is_none());
}
