//! Administrators acting at the same time. Each change checks its rules on
//! the stored data under the company's administration lock, not on the view
//! its request loaded: crossed moves cannot build a cycle in the tree (which
//! would lock the whole company out), crossed revocations cannot leave the
//! company without an administrator.

mod support;

use axum::http::StatusCode;
use mimorya_api::services::admin::{
    UnitChanges, UserChanges, revoke_role, update_unit, update_user,
};
use mimorya_api::services::{AccessContext, AuthIdentity};
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_core::{DomainError, ErrorCode};
use serde_json::json;
use support::accounts::{assignment_of, create_account};
use support::{Client, TestApi};
use uuid::Uuid;

/// The authorization context of a request, as loaded when it started.
async fn context_of(api: &TestApi, user_id: Uuid) -> AccessContext {
    let identity = AuthIdentity {
        tenant_id: tenant::ID,
        user_id,
        session_id: Uuid::nil(),
    };
    api.app.services.access.context(identity).await.unwrap()
}

fn code_of(result: anyhow::Result<()>) -> ErrorCode {
    result
        .expect_err("refused")
        .downcast::<DomainError>()
        .expect("domain error")
        .code
}

async fn new_team(paul: &Client<'_>, name: &str) -> Uuid {
    let created = paul
        .post(
            "/admin/org-units",
            json!({ "parentId": unit::TECHNICAL, "name": name, "type": "team" }),
        )
        .await;
    assert_eq!(created.status, StatusCode::CREATED);
    created.json()["id"].as_str().unwrap().parse().unwrap()
}

fn move_under(parent: Uuid) -> UnitChanges {
    UnitChanges {
        parent_id: Some(parent),
        ..UnitChanges::default()
    }
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn crossed_moves_keep_the_tree_a_tree() {
    let api = TestApi::start().await;
    let services = &api.app.services;
    let paul = api.sign_in(user::PAUL).await;
    let marc = api.sign_in(user::MARC).await;

    // Two requests that loaded the same tree, then move X under Y and Y under X.
    let (x, y) = (
        new_team(&paul, "Équipe X").await,
        new_team(&paul, "Équipe Y").await,
    );
    let first = context_of(&api, user::PAUL).await;
    let second = context_of(&api, user::PAUL).await;
    update_unit(&services.db, &services.access, &first, x, move_under(y))
        .await
        .unwrap();
    let crossed = update_unit(&services.db, &services.access, &second, y, move_under(x)).await;
    assert_eq!(code_of(crossed), ErrorCode::Conflict);

    // The same race over HTTP: one move wins, the other is refused.
    let (a, b) = (
        new_team(&paul, "Équipe A").await,
        new_team(&paul, "Équipe B").await,
    );
    let (url_a, url_b) = (
        format!("/admin/org-units/{a}"),
        format!("/admin/org-units/{b}"),
    );
    let (one, other) = tokio::join!(
        paul.patch(&url_a, json!({ "parentId": b })),
        paul.patch(&url_b, json!({ "parentId": a })),
    );
    let mut statuses = [one.status, other.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);

    assert_eq!(marc.get("/auth/me").await.status, StatusCode::OK);
    assert_eq!(paul.get("/admin/organization").await.status, StatusCode::OK);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn crossed_revocations_leave_an_administrator() {
    let api = TestApi::start().await;
    let services = &api.app.services;
    let paul = api.sign_in(user::PAUL).await;
    let account = create_account(&paul, "second.admin").await;
    let grant = json!({ "roleId": role::TENANT_ADMIN, "scope": { "type": "tenant" } });
    let grants = format!("/admin/users/{}/assignments", account.id);
    assert_eq!(
        paul.post(&grants, grant.clone()).await.status,
        StatusCode::CREATED
    );

    // Each administrator removes the other's role, from views loaded before.
    let from_paul = context_of(&api, user::PAUL).await;
    let from_second = context_of(&api, account.id).await;
    let second_role = assignment_of(&paul, account.id, role::TENANT_ADMIN)
        .await
        .parse()
        .unwrap();
    let paul_role = assignment_of(&paul, user::PAUL, role::TENANT_ADMIN)
        .await
        .parse()
        .unwrap();
    revoke_role(&services.db, &from_paul, second_role)
        .await
        .unwrap();
    let crossed = revoke_role(&services.db, &from_second, paul_role).await;
    assert_eq!(code_of(crossed), ErrorCode::Forbidden);
    assert_eq!(
        paul.get("/admin/users").await.json()["canManage"]["roles"],
        true
    );

    // Same with crossed disabling.
    assert_eq!(paul.post(&grants, grant).await.status, StatusCode::CREATED);
    let from_paul = context_of(&api, user::PAUL).await;
    let from_second = context_of(&api, account.id).await;
    let disable = || UserChanges {
        active: Some(false),
        ..UserChanges::default()
    };
    update_user(&services.db, &from_paul, account.id, disable())
        .await
        .unwrap();
    let crossed = update_user(&services.db, &from_second, user::PAUL, disable()).await;
    assert_eq!(code_of(crossed), ErrorCode::Forbidden);
    assert_eq!(paul.get("/auth/me").await.status, StatusCode::OK);
}
