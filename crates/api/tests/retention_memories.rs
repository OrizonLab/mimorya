//! Retention applied to the memories (DATA_MODEL §5, ADR-185). A pass
//! deletes the episodes recorded a year ago and the memories past their own
//! term (`aged`), the personal memories of an account disabled thirty days
//! ago (`departed`; one both is `aged`); what is more recent, or an account
//! enabled again, stays. Each company that lost memories finds it in its own
//! audit log, by the platform, counted, never read; a pass with nothing to
//! delete tells no one. The test starts a process without maintenance: it
//! runs the passes itself.

mod support;

use mimorya_api::plugins::retention::apply_retention;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::audit_exports::audit_log;
use support::queue::API;
use support::retention::{account, disabled_days_ago, memory, owner, purged_entries, titles};
use support::{TestApi, companies};
use uuid::Uuid;

const PURGED: &str = "memory.purged";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn memories_past_their_retention_go_each_company_told() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let paul = api.sign_in(user::PAUL).await;
    let neo = tenant::ID;
    let gone = account(neo, "parti.retention@neo-maintenance.example", "disabled").await;
    let leaving = account(neo, "partant.retention@neo-maintenance.example", "disabled").await;
    let back = account(neo, "revenu.retention@neo-maintenance.example", "disabled").await;
    disabled_days_ago(gone, 31).await;
    disabled_days_ago(leaving, 29).await;
    let owner = owner().await;
    sqlx::query("update users set status = 'active' where id = $1")
        .bind(back)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;

    memory(
        neo,
        "rétention épisode 13 mois",
        "episode",
        None,
        396,
        false,
    )
    .await;
    memory(
        neo,
        "rétention épisode 11 mois",
        "episode",
        None,
        335,
        false,
    )
    .await;
    memory(neo, "rétention fait expiré", "fact", None, 1, true).await;
    memory(neo, "rétention fait ancien", "fact", None, 900, false).await;
    memory(neo, "rétention parti", "preference", Some(gone), 1, false).await;
    // Both aged and departed: counted aged.
    memory(
        neo,
        "rétention parti épisode",
        "episode",
        Some(gone),
        400,
        false,
    )
    .await;
    memory(
        neo,
        "rétention partant",
        "preference",
        Some(leaving),
        1,
        false,
    )
    .await;
    memory(neo, "rétention revenu", "preference", Some(back), 1, false).await;
    memory(
        neo,
        "rétention Paul",
        "preference",
        Some(user::PAUL),
        400,
        false,
    )
    .await;
    let acme = companies::create(&api, "acme-retention", "admin@acme-retention.example").await;
    memory(
        acme.tenant_id,
        "rétention Acme",
        "episode",
        None,
        400,
        false,
    )
    .await;

    let retained = apply_retention(db).await.unwrap().unwrap();
    let lost = |tenant_id: Uuid| {
        retained
            .memories
            .iter()
            .find(|lost| lost.tenant_id == tenant_id)
            .map(|lost| (lost.aged, lost.departed))
    };
    assert_eq!(lost(neo), Some((3, 1)));
    assert_eq!(lost(acme.tenant_id), Some((1, 0)));
    assert_eq!(
        titles("rétention").await,
        [
            "rétention Paul",
            "rétention fait ancien",
            "rétention partant",
            "rétention revenu",
            "rétention épisode 11 mois",
        ]
    );

    // Each company told in its own log, by the platform, of its counts alone.
    let told: Vec<_> = audit_log(&paul)
        .await
        .into_iter()
        .filter(|entry| entry["action"] == "memory.purged")
        .collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0]["actorType"], "system");
    assert_eq!(told[0]["metadata"], json!({ "aged": 3, "departed": 1 }));
    assert_eq!(
        purged_entries(acme.tenant_id, PURGED).await,
        [json!({ "aged": 1, "departed": 0 })]
    );

    // Nothing more to delete: nothing more told.
    let again = apply_retention(db).await.unwrap().unwrap();
    assert!(again.memories.is_empty(), "{again:?}");
    assert_eq!(purged_entries(neo, PURGED).await.len(), 1);
    assert_eq!(purged_entries(acme.tenant_id, PURGED).await.len(), 1);
}
