//! Administrators acting at the same time. Each change checks its rules on
//! the stored data under the company's administration lock, not on the view
//! its request loaded: crossed moves cannot build a cycle in the tree (which
//! would lock the whole company out), crossed revocations cannot leave the
//! company without an administrator, and a unit being deleted cannot take
//! away what was written into it meanwhile. Races are made deterministic by
//! holding row locks in a test transaction while the requests run.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_api::services::admin::{
    UnitChanges, UserChanges, revoke_role, update_unit, update_user,
};
use mimorya_api::services::{AccessContext, AuthIdentity};
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{role, tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_core::{DomainError, ErrorCode};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
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

    // The same race over HTTP, truly concurrent: the units' rows are held by
    // the test, so without the administration lock both requests would check
    // the same tree, wait on their update, then both succeed.
    let (a, b) = (
        new_team(&paul, "Équipe A").await,
        new_team(&paul, "Équipe B").await,
    );
    let mut hold = services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("select id from org_units where id = any($1) for update")
        .bind(vec![a, b])
        .fetch_all(&mut *hold)
        .await
        .unwrap();
    let (url_a, url_b) = (
        format!("/admin/org-units/{a}"),
        format!("/admin/org-units/{b}"),
    );
    let (one, other, ()) = tokio::join!(
        paul.patch(&url_a, json!({ "parentId": b })),
        paul.patch(&url_b, json!({ "parentId": a })),
        async {
            tokio::time::sleep(Duration::from_millis(500)).await;
            hold.commit().await.unwrap();
        },
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn deleting_a_unit_waits_for_what_is_being_written_into_it() {
    let api = TestApi::start().await;
    let services = &api.app.services;
    let paul = api.sign_in(user::PAUL).await;
    let id = new_team(&paul, "Équipe éphémère").await;

    // A run writes a team memory into the unit; it commits during the deletion.
    let mut writing = services.db.begin(tenant::ID).await.unwrap();
    let memory = create_memory(
        &mut writing,
        NewMemory {
            tenant_id: tenant::ID,
            scope: MemoryScope::OrgUnit,
            owner_user_id: None,
            org_unit_id: Some(id),
            kind: MemoryKind::Lesson,
            title: "Accès au chantier".into(),
            content: "Prévenir le gardien la veille.".into(),
            sensitivity: Sensitivity::Internal,
            status: MemoryStatus::Active,
            source: MemorySource::manual(None),
            created_by_user_id: Some(user::PAUL),
            created_by_run_id: None,
        },
    )
    .await
    .unwrap();
    let url = format!("/admin/org-units/{id}");
    let (deleted, ()) = tokio::join!(paul.delete(&url), async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        writing.commit().await.unwrap();
    });
    assert_eq!(deleted.status, StatusCode::CONFLICT, "{}", deleted.text());

    let mut check = services.db.begin(tenant::ID).await.unwrap();
    let kept: i64 = sqlx::query_scalar("select count(*) from memory_items where id = $1")
        .bind(memory)
        .fetch_one(&mut *check)
        .await
        .unwrap();
    assert_eq!(kept, 1, "the memory is not dropped by a cascade");
}
