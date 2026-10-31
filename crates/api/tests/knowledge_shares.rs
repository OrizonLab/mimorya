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
use serde_json::json;
use support::TestApi;
use support::documents::{Target, deposited, listed, replace};
use support::knowledge_shares::{SHARES, memories, north_to_south, share, team_memory, unshare};
use support::queue::as_owner;
use support::refusals::{entries_since, latest_entry};
use uuid::Uuid;

const TEXT: &[u8] = b"Consigne : purger le circuit avant intervention.";

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
    let made = share(&paul, north_to_south((true, false), "public")).await;
    assert!(!memories(&hugo).await.contains(&north));
    unshare(&paul, &made).await;
    let audited = entries_since(&api, since).await;
    assert!(audited.contains("knowledge_share.created"), "{audited}");
    assert!(audited.contains("knowledge_share.deleted"), "{audited}");

    // Its memory, up to confidential: what Hugo's clearance (internal)
    // reads of it, never deleted; never its documents.
    let made = share(&paul, north_to_south((true, false), "confidential")).await;
    let read = memories(&hugo).await;
    assert!(read.contains(&north), "{read:?}");
    assert!(!read.contains(&secret), "{read:?}");
    let refused = hugo.delete(&format!("/memory/{north}")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    assert!(memories(&julien).await.contains(&north));
    assert!(!listed(&hugo).await.contains(&document));
    let absent = hugo.get(&format!("/documents/{document}")).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);
    // One way only: the south's memory stays the south's.
    assert!(!memories(&lucas).await.contains(&south));
    unshare(&paul, &made).await;

    // Its documents too: read, never deleted nor replaced.
    let made = share(&paul, north_to_south((true, true), "internal")).await;
    assert!(listed(&hugo).await.contains(&document));
    let read = hugo.get(&format!("/documents/{document}")).await;
    assert_eq!(read.status, StatusCode::OK, "{}", read.text());
    assert_eq!(read.json()["canWrite"], false);
    let refused = hugo.delete(&format!("/documents/{document}")).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);
    let refused = replace(&hugo, document, "Sud.txt", 1, b"Autre consigne").await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{}", refused.text());

    // Removed: nothing more read.
    unshare(&paul, &made).await;
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
