//! Knowledge shared between units (ADR-143): the north team's knowledge
//! shared with the south team is read by its members as their own, the
//! memory or the documents as shared, never above the share's sensitivity
//! nor their clearance, never written or deleted; nothing else crosses (the
//! south's knowledge stays the south's), and nothing once the share is
//! removed or past its date.

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{tenant, unit, user};
use mimorya_core::memory::{MemoryKind, MemoryScope, MemorySource, MemoryStatus};
use mimorya_db::repositories::memory::{NewMemory, create_memory};
use serde_json::json;
use support::documents::{Target, deposited, listed};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use support::{Client, TestApi};
use uuid::Uuid;

const SHARES: &str = "/admin/knowledge-shares";
const TEXT: &[u8] = b"Consigne : purger le circuit avant intervention.";

/// A memory of `unit`'s space, active, of `sensitivity`.
async fn team_memory(api: &TestApi, unit_id: Uuid, sensitivity: Sensitivity) -> String {
    let mut tx = api.app.services.db.begin(tenant::ID).await.unwrap();
    let id = create_memory(
        &mut tx,
        NewMemory {
            tenant_id: tenant::ID,
            scope: MemoryScope::OrgUnit,
            owner_user_id: None,
            org_unit_id: Some(unit_id),
            kind: MemoryKind::Fact,
            title: format!("Consigne {}", sensitivity.as_str()),
            content: "Demander le badge au gardien la veille.".into(),
            sensitivity,
            status: MemoryStatus::Active,
            source: MemorySource::manual(None),
            created_by_user_id: Some(user::JULIEN),
            created_by_run_id: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    id.to_string()
}

/// The memories `client` reads.
async fn memories(client: &Client<'_>) -> Vec<String> {
    let items = client.get("/memory").await.json()["items"].clone();
    (items.as_array().unwrap().iter())
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect()
}

/// Shares the north team's knowledge with the south team as `paul`: the
/// memory, the documents, up to `max`; the share.
async fn share_north(paul: &Client<'_>, (memory, documents): (bool, bool), max: &str) -> String {
    let body = json!({
        "sourceOrgUnitId": unit::TEAM_NORTH,
        "targetOrgUnitId": unit::TEAM_SOUTH,
        "memory": memory,
        "documents": documents,
        "maxSensitivity": max,
    });
    let created = paul.post(SHARES, body).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    created.json()["id"].as_str().unwrap().to_owned()
}

async fn unshare(paul: &Client<'_>, id: &str) {
    let removed = paul.delete(&format!("{SHARES}/{id}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_team_reads_what_another_shares_with_it_and_only_that() {
    let api = TestApi::start().await;
    let (paul, julien) = (
        api.sign_in(user::PAUL).await,
        api.sign_in(user::JULIEN).await,
    );
    let (hugo, lucas) = (
        api.sign_in(user::HUGO).await,
        api.sign_in(user::LUCAS).await,
    );
    let north = team_memory(&api, unit::TEAM_NORTH, Sensitivity::Internal).await;
    let secret = team_memory(&api, unit::TEAM_NORTH, Sensitivity::Confidential).await;
    let south = team_memory(&api, unit::TEAM_SOUTH, Sensitivity::Internal).await;
    let document = deposited(&julien, "Nord.txt", Target::unit(unit::TEAM_NORTH), TEXT).await;
    assert!(!memories(&hugo).await.contains(&north));
    assert!(!listed(&hugo).await.contains(&document));

    // Shared up to public: nothing of his read.
    let since = latest_entry(&api).await;
    let share = share_north(&paul, (true, false), "public").await;
    assert!(!memories(&hugo).await.contains(&north));
    unshare(&paul, &share).await;
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("knowledge_share.created"), "{audited}");
    assert!(audited.contains("knowledge_share.deleted"), "{audited}");

    // Its memory, up to confidential: what Hugo's clearance (internal)
    // reads of it; never its documents.
    let share = share_north(&paul, (true, false), "confidential").await;
    let read = memories(&hugo).await;
    assert!(read.contains(&north), "{read:?}");
    assert!(!read.contains(&secret), "{read:?}");
    assert!(!listed(&hugo).await.contains(&document));
    let absent = hugo.get(&format!("/documents/{document}")).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    // One way only: the south's memory stays the south's.
    assert!(!memories(&lucas).await.contains(&south));
    unshare(&paul, &share).await;

    // Its documents too: read, never deleted.
    let share = share_north(&paul, (true, true), "internal").await;
    assert!(listed(&hugo).await.contains(&document));
    let read = hugo.get(&format!("/documents/{document}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    assert_eq!(read.json()["canDelete"], false);
    let refused = hugo.delete(&format!("/documents/{document}")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);

    // Removed: nothing more read.
    unshare(&paul, &share).await;
    assert!(!memories(&hugo).await.contains(&north));
    assert!(!listed(&hugo).await.contains(&document));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_share_reaches_the_units_below_only_if_said_and_ends_at_its_date() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let hugo = api.sign_in(user::HUGO).await;
    let direction = team_memory(&api, unit::DIRECTION, Sensitivity::Internal).await;
    let shared = |below: bool| {
        json!({
            "sourceOrgUnitId": unit::DIRECTION,
            "targetOrgUnitId": unit::TECHNICAL,
            "includeDescendants": below,
            "memory": true,
            "documents": false,
            "maxSensitivity": "internal",
            "expiresAt": "2999-01-01T00:00:00Z",
        })
    };

    // Shared with the technical service: not with its teams.
    let created = paul.post(SHARES, shared(false)).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id = created.json()["id"].as_str().unwrap().to_owned();
    assert!(!memories(&hugo).await.contains(&direction));
    unshare(&paul, &id).await;
    // With the units under it too: read by the south team's members.
    let created = paul.post(SHARES, shared(true)).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let id: Uuid = created.json()["id"].as_str().unwrap().parse().unwrap();
    assert!(memories(&hugo).await.contains(&direction));
    let listed = paul.get(SHARES).await.json();
    assert_eq!(listed["shares"][0]["includeDescendants"], true);
    assert_eq!(
        listed["shares"][0]["expiresAt"],
        "2999-01-01T00:00:00+00:00"
    );

    // Past its date: read no more.
    as_owner(
        "update knowledge_shares set expires_at = now() - interval '1 minute' where id = $1",
        id,
    )
    .await;
    api.app.services.access.invalidate(tenant::ID);
    assert!(!memories(&hugo).await.contains(&direction));
}
