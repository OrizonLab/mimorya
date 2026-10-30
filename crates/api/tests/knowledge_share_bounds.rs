//! What a share never opens (ADR-143): nothing to whom a deny keeps from
//! reading their own unit's knowledge, nothing of the units under the unit
//! shared, and no one's personal memory, even filed with that unit.

mod support;

use axum::http::StatusCode;
use mimorya_core::authz::Sensitivity;
use mimorya_core::demo::{unit, user};
use serde_json::json;
use support::TestApi;
use support::documents::{Target, deposited, listed, remove};
use support::knowledge_shares::{
    memories, north_to_south, personal_memory, share, share_of, team_memory, unshare,
};

const TEXT: &[u8] = b"Consigne : couper l'alimentation avant intervention.";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn whom_a_deny_keeps_from_reading_gains_nothing_by_a_share() {
    let api = TestApi::start().await;
    let (paul, julien) = (
        api.sign_in(user::PAUL).await,
        api.sign_in(user::JULIEN).await,
    );
    let (hugo, lea) = (api.sign_in(user::HUGO).await, api.sign_in(user::LEA).await);
    let north = team_memory(&api, unit::TEAM_NORTH, Sensitivity::Internal).await;
    let document = deposited(&julien, "Nord.txt", Target::unit(unit::TEAM_NORTH), TEXT).await;
    let only_hugo = json!({ "op": "eq", "left": { "attr": "subject.userId" }, "right": { "value": user::HUGO } });
    let policy = json!({
        "description": "Ni mémoire ni documents",
        "permissions": ["memory:read", "document:read"],
        "condition": only_hugo,
    });
    let created = paul.post("/admin/policies", policy).await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.text());
    let policy = created.json()["id"].as_str().unwrap().to_owned();

    let made = share(&paul, north_to_south((true, true), "internal")).await;
    // His team-mate reads it; not him.
    assert!(memories(&lea).await.contains(&north));
    assert!(listed(&lea).await.contains(&document));
    assert!(!memories(&hugo).await.contains(&north));
    assert!(!listed(&hugo).await.contains(&document));
    let absent = hugo.get(&format!("/documents/{document}")).await;
    assert_eq!(absent.status, StatusCode::NOT_FOUND);

    unshare(&paul, &made).await;
    let removed = paul.delete(&format!("/admin/policies/{policy}")).await;
    assert_eq!(removed.status, StatusCode::OK, "{}", removed.text());
    remove(&julien, document).await;
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_share_opens_the_unit_s_space_not_those_below_nor_anyone_s_own() {
    let api = TestApi::start().await;
    let paul = api.sign_in(user::PAUL).await;
    let (claire, hugo) = (
        api.sign_in(user::CLAIRE).await,
        api.sign_in(user::HUGO).await,
    );
    let technical = team_memory(&api, unit::TECHNICAL, Sensitivity::Internal).await;
    let north = team_memory(&api, unit::TEAM_NORTH, Sensitivity::Internal).await;
    let own = personal_memory(&api, user::JULIEN, unit::TEAM_NORTH).await;
    assert!(!memories(&claire).await.contains(&technical));

    // The technical service's memory, not its teams'.
    let units = (unit::TECHNICAL, unit::DIRECTION);
    let made = share(&paul, share_of(units, (true, false), "internal")).await;
    let read = memories(&claire).await;
    assert!(read.contains(&technical), "{read:?}");
    assert!(!read.contains(&north), "{read:?}");
    unshare(&paul, &made).await;

    // The north team's memory, not Julien's own filed with it.
    let made = share(&paul, north_to_south((true, false), "restricted")).await;
    let read = memories(&hugo).await;
    assert!(read.contains(&north), "{read:?}");
    assert!(!read.contains(&own), "{read:?}");
    unshare(&paul, &made).await;
}
