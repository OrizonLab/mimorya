//! Role assignments, and the limits of an administrator's power: no change to
//! their own rights, administration roles company-wide only, no duplicate,
//! nothing reachable in another company, and nothing at all for an
//! administrator whose role only covers one team.

mod support;

use axum::http::StatusCode;
use mimorya_core::demo::{role, unit, user};
use mimorya_db::testing::OTHER_USER_ID;
use serde_json::json;
use support::TestApi;
use support::accounts::{
    activate, assignment_of, create_account, create_partial_admin, permissions,
};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn grants_roles_on_their_scope_and_revokes_them() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;
    let account = create_account(&paul, "futur.chef").await;
    let session = api.sign_in_with(&account.email, &account.password).await;
    let grants = format!("/admin/users/{}/assignments", account.id);

    let team_lead = json!({
        "roleId": role::TEAM_LEAD,
        "scope": { "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true },
    });
    assert_eq!(
        marc.post(&grants, team_lead.clone()).await.status,
        StatusCode::FORBIDDEN
    );
    let granted = paul.post(&grants, team_lead.clone()).await;
    assert_eq!(granted.status, StatusCode::CREATED);
    let assignment = granted.json()["id"].as_str().unwrap().to_string();
    let me = session.get("/auth/me").await.json();
    assert!(permissions(&me).contains(&"planning:apply".to_string()));
    // The same role on the same scope is not held twice.
    assert_eq!(
        paul.post(&grants, team_lead).await.status,
        StatusCode::CONFLICT
    );

    // Administration roles cover the whole company or nothing.
    let admin_on_unit = json!({
        "roleId": role::TENANT_ADMIN,
        "scope": { "type": "org_unit", "orgUnitId": unit::TEAM_SOUTH, "includeDescendants": true },
    });
    assert_eq!(
        paul.post(&grants, admin_on_unit).await.status,
        StatusCode::BAD_REQUEST
    );
    let unknown_unit = json!({
        "roleId": role::TEAM_LEAD,
        "scope": { "type": "org_unit", "orgUnitId": uuid::Uuid::new_v4(), "includeDescendants": true },
    });
    assert_eq!(
        paul.post(&grants, unknown_unit).await.status,
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
    let me = session.get("/auth/me").await.json();
    assert!(!permissions(&me).contains(&"planning:apply".to_string()));
    assert_eq!(paul.delete(&revoke).await.status, StatusCode::NOT_FOUND);
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
    let own_admin_role = assignment_of(&paul, user::PAUL, role::TENANT_ADMIN).await;
    let revoke = paul
        .delete(&format!("/admin/assignments/{own_admin_role}"))
        .await;
    assert_eq!(revoke.status, StatusCode::FORBIDDEN);
    assert_eq!(
        paul.get("/admin/users").await.json()["canManage"]["roles"],
        true
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn grants_nothing_to_an_account_of_another_company() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let grant = paul
        .post(
            &format!("/admin/users/{OTHER_USER_ID}/assignments"),
            json!({ "roleId": role::DIRECTOR, "scope": { "type": "tenant" } }),
        )
        .await;
    assert_eq!(grant.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn honours_administration_permissions_on_the_whole_company_only() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let mut account = create_partial_admin(&api, &paul, "admin.partiel").await;
    // Its own password chosen: every refusal below comes from the scope rule.
    let partial = activate(&api, &mut account).await;
    let me = partial.get("/auth/me").await.json();
    for permission in ["user:manage", "role:manage", "org:manage", "audit:read"] {
        assert!(
            !permissions(&me).contains(&permission.to_string()),
            "{permission} is not announced on one team"
        );
    }

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

    // Every change is refused, even on the team the role covers.
    let lucas = format!("/admin/users/{}", user::LUCAS);
    let lucas_member_role = assignment_of(&paul, user::LUCAS, role::TECHNICIAN).await;
    let attempts = [
        partial
            .post(
                "/admin/users",
                json!({ "email": "y@neo-maintenance.example", "displayName": "Yves Essai" }),
            )
            .await,
        partial.patch(&lucas, json!({ "status": "disabled" })).await,
        partial.post(&format!("{lucas}/password"), json!({})).await,
        partial
            .post(
                &format!("{lucas}/assignments"),
                json!({ "roleId": role::TENANT_ADMIN, "scope": { "type": "tenant" } }),
            )
            .await,
        partial
            .delete(&format!("/admin/assignments/{lucas_member_role}"))
            .await,
        partial
            .post(
                "/admin/org-units",
                json!({ "parentId": unit::TEAM_NORTH, "name": "Cellule", "type": "team" }),
            )
            .await,
        partial
            .patch(
                &format!("/admin/org-units/{}", unit::TEAM_NORTH),
                json!({ "name": "Équipe Nord renommée" }),
            )
            .await,
        partial
            .delete(&format!("/admin/org-units/{}", unit::TEAM_NORTH))
            .await,
    ];
    for (index, attempt) in attempts.iter().enumerate() {
        assert_eq!(
            attempt.status,
            StatusCode::FORBIDDEN,
            "attempt {index}: {}",
            attempt.text()
        );
    }
}
