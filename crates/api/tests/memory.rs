//! Memory rights through the API: personal memory stays personal.

mod support;

use std::time::Duration;

use axum::http::StatusCode;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use serde_json::{Value, json};
use support::TestApi;
use uuid::Uuid;

fn find<'a>(items: &'a [Value], title: &str) -> &'a Value {
    items
        .iter()
        .find(|i| i["title"] == title)
        .unwrap_or_else(|| panic!("memory {title} not found"))
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn lets_users_see_and_erase_their_own_memory_never_other_peoples() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let marc = api.sign_in(user::MARC).await;
    let items = lucas.get("/memory").await.json()["items"]
        .as_array()
        .unwrap()
        .clone();
    assert!(!items.iter().any(|i| i["title"] == "Format des synthèses"));
    let own = find(&items, "Prévenance des déplacements");
    assert_eq!(own["canManage"], true);
    let own_url = format!("/memory/{}", own["id"].as_str().unwrap());

    assert_eq!(marc.delete(&own_url).await.status, StatusCode::NOT_FOUND);
    assert_eq!(lucas.delete(&own_url).await.status, StatusCode::OK);
    let after = lucas.get("/memory").await.json();
    assert!(
        !after["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["id"] == own["id"])
    );

    let shared = find(&items, "Retours chantier équipe Nord");
    assert_eq!(shared["canManage"], false);
    let shared_url = format!("/memory/{}", shared["id"].as_str().unwrap());
    assert_eq!(
        lucas.delete(&shared_url).await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn writes_only_into_spaces_the_user_may_write() {
    let api = TestApi::start().await;
    let lucas = api.sign_in(user::LUCAS).await;
    let entry = |space: &str| {
        json!({
            "space": space,
            "kind": "preference",
            "title": "  Format court  ",
            "content": "Réponses en trois points maximum.",
        })
    };
    let personal = lucas.post("/memory", entry("personal")).await;
    assert_eq!(personal.status, StatusCode::CREATED);
    let company = lucas.post("/memory", entry("company")).await;
    assert_eq!(company.status, StatusCode::FORBIDDEN);

    let items = lucas.get("/memory").await.json();
    let created = items["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == personal.json()["id"])
        .cloned()
        .expect("created memory listed");
    assert_eq!(created["title"], "Format court");
    assert_eq!(created["scope"], "user");

    let invalid = lucas
        .post(
            "/memory",
            json!({ "space": "personal", "kind": "preference", "title": "x", "content": "y" }),
        )
        .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
}

/// A team memory proposed for the north team, as the agent records it.
async fn propose_team_memory(api: &TestApi) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = create_memory(
        &mut tx,
        NewMemory {
            tenant_id: tenant::ID,
            scope: MemoryScope::OrgUnit,
            owner_user_id: None,
            org_unit_id: Some(unit::TEAM_NORTH),
            kind: MemoryKind::Lesson,
            title: "Accès au local technique".into(),
            content: "Demander le badge au gardien la veille.".into(),
            sensitivity: Sensitivity::Internal,
            status: MemoryStatus::Proposed,
            source: MemorySource::manual(None),
            created_by_user_id: Some(user::LUCAS),
            created_by_run_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id.to_string()
}

fn status_of(listing: &Value, id: &str) -> Option<String> {
    listing["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .map(|i| i["status"].as_str().unwrap().to_string())
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn shows_and_settles_proposals_only_for_those_who_may_write_the_space() {
    let api = TestApi::start().await;
    let id = propose_team_memory(&api).await;
    let lucas = api.sign_in(user::LUCAS).await;
    let julien = api.sign_in(user::JULIEN).await;
    let review = format!("/memory/{id}/review");

    assert_eq!(status_of(&lucas.get("/memory").await.json(), &id), None);
    let refused = lucas.post(&review, json!({ "decision": "approve" })).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);

    let listing = julien.get("/memory").await.json();
    assert_eq!(status_of(&listing, &id).as_deref(), Some("proposed"));
    let approved = julien.post(&review, json!({ "decision": "approve" })).await;
    assert_eq!(approved.status, StatusCode::OK);
    assert_eq!(
        status_of(&lucas.get("/memory").await.json(), &id).as_deref(),
        Some("active")
    );
    let again = julien.post(&review, json!({ "decision": "reject" })).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn settles_a_proposal_once_when_decisions_cross() {
    let api = TestApi::start().await;
    let id = propose_team_memory(&api).await;
    let julien = api.sign_in(user::JULIEN).await;

    // Another manager is approving the proposal at the same moment.
    let mut concurrent = api.app.services.db.begin(tenant::ID).await.unwrap();
    sqlx::query("update memory_items set status = 'active' where id = $1")
        .bind(id.parse::<Uuid>().unwrap())
        .execute(&mut *concurrent)
        .await
        .unwrap();
    let review = format!("/memory/{id}/review");
    let mut rejecting = Box::pin(julien.post(&review, json!({ "decision": "reject" })));
    let waited = tokio::time::timeout(Duration::from_millis(300), &mut rejecting).await;
    assert!(
        waited.is_err(),
        "the second decision must wait for the first"
    );
    concurrent.commit().await.unwrap();

    assert_eq!(rejecting.await.status, StatusCode::NOT_FOUND);
    let listing = julien.get("/memory").await.json();
    assert_eq!(status_of(&listing, &id).as_deref(), Some("active"));
}
