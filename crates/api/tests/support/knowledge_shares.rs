//! Knowledge shared between units (ADR-143), as the tests make it: memories
//! filed in a unit's space or someone's own, what a member reads of the
//! memory, shares made and removed by the policies' administrator.

use axum::http::StatusCode;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Client, TestApi};

pub const SHARES: &str = "/admin/knowledge-shares";

/// A memory of `unit_id`'s space, active, of `sensitivity`.
pub async fn team_memory(api: &TestApi, unit_id: Uuid, sensitivity: Sensitivity) -> String {
    let title = format!("Consigne {}", sensitivity.as_str());
    remember(
        api,
        (MemoryScope::OrgUnit, None),
        unit_id,
        (title, sensitivity),
    )
    .await
}

/// A personal memory of `owner`'s, filed with `unit_id` all the same.
pub async fn personal_memory(api: &TestApi, owner: Uuid, unit_id: Uuid) -> String {
    let what = ("Note personnelle".into(), Sensitivity::Internal);
    remember(api, (MemoryScope::User, Some(owner)), unit_id, what).await
}

async fn remember(
    api: &TestApi,
    (scope, owner_user_id): (MemoryScope, Option<Uuid>),
    unit_id: Uuid,
    (title, sensitivity): (String, Sensitivity),
) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = create_memory(
        &mut tx,
        NewMemory {
            tenant_id: tenant::ID,
            scope,
            owner_user_id,
            org_unit_id: Some(unit_id),
            kind: MemoryKind::Fact,
            title,
            content: "Demander le badge au gardien la veille.".into(),
            sensitivity,
            status: MemoryStatus::Active,
            source: MemorySource::manual(None),
            created_by_user_id: Some(owner_user_id.unwrap_or(user::JULIEN)),
            created_by_run_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id.to_string()
}

/// The memories `client` reads.
pub async fn memories(client: &Client<'_>) -> Vec<String> {
    let items = client.get("/memory").await.json()["items"].clone();
    (items.as_array().unwrap().iter())
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

/// `source`'s knowledge shared with `target`: the memory, the documents, up
/// to `max`.
pub fn share_of(
    (source, target): (Uuid, Uuid),
    (memory, documents): (bool, bool),
    max: &str,
) -> Value {
    json!({
        "sourceOrgUnitId": source,
        "targetOrgUnitId": target,
        "memory": memory,
        "documents": documents,
        "maxSensitivity": max,
    })
}

/// The north team's knowledge shared with the south team.
pub fn north_to_south(knowledge: (bool, bool), max: &str) -> Value {
    share_of((unit::TEAM_NORTH, unit::TEAM_SOUTH), knowledge, max)
}

/// Makes the share `body` as `paul`; the share.
pub async fn share(paul: &Client<'_>, body: Value) -> String {
    let created = paul.post(SHARES, body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_owned()
}

pub async fn unshare(paul: &Client<'_>, id: &str) {
    let removed = paul.delete(&format!("{SHARES}/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}
