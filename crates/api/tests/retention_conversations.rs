//! Retention applied to the conversations (DATA_MODEL §5, ADR-186). A pass
//! deletes the conversations idle for twenty-four months, whole: their
//! messages, runs and sub-agents' runs, events, tool calls and validations.
//! A conversation more recent, or with a run still awaiting a validation,
//! stays. Each company that lost conversations finds it in its own audit
//! log, by the platform, counted, never read; a pass with nothing to delete
//! tells no one. An audit entry naming a deleted run keeps naming it, and
//! the company's sealed chain holds. The test starts a process without
//! maintenance: it runs the passes and the sealing itself.

mod support;

use mimorya_api::plugins::retention::apply_retention;
use mimorya_api::services::audit_chain::SealedHeads;
use mimorya_core::demo::{tenant, user};
use serde_json::json;
use support::audit_chain::{chain, intact, seal, verify};
use support::audit_exports::audit_log;
use support::idle_conversations::{
    audited_run, conversation, conversations_left, left_of_run, named_run, run,
};
use support::queue::API;
use support::retention::purged_entries;
use support::{TestApi, companies};
use uuid::Uuid;

const PURGED: &str = "conversation.purged";

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn conversations_idle_two_years_go_whole_each_company_told() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let paul = api.sign_in(user::PAUL).await;
    let neo = tenant::ID;

    let idle = conversation(neo, user::MARC, "rétention conversation 25 mois", 25).await;
    let idle_run = run(neo, idle, user::MARC, "completed").await;
    let recent = conversation(neo, user::MARC, "rétention conversation 23 mois", 23).await;
    let recent_run = run(neo, recent, user::MARC, "completed").await;
    let live = conversation(neo, user::MARC, "rétention conversation en attente", 25).await;
    let live_run = run(neo, live, user::MARC, "awaiting_approval").await;
    let acme = companies::create(
        &api,
        "acme-conversations",
        "admin@acme-conversations.example",
    )
    .await;
    let acme_idle = conversation(
        acme.tenant_id,
        acme.admin_id,
        "rétention conversation Acme",
        30,
    )
    .await;
    run(acme.tenant_id, acme_idle, acme.admin_id, "failed").await;

    // The idle run, named in the company's audit log, and the chain sealed.
    let entry = audited_run(db, neo, idle_run).await;
    let heads = SealedHeads::default();
    seal(db, &heads, neo).await.unwrap();
    let sealed = chain(db, neo).await.len() as i64;
    assert!(chain(db, neo).await.iter().any(|(id, _)| *id == entry));

    let retained = apply_retention(db).await.unwrap().unwrap();
    let lost = |tenant_id: Uuid| {
        retained
            .conversations
            .iter()
            .find(|lost| lost.tenant_id == tenant_id)
            .map(|lost| (lost.conversations, lost.messages))
    };
    assert_eq!(lost(neo), Some((1, 2)));
    assert_eq!(lost(acme.tenant_id), Some((1, 2)));
    assert_eq!(
        conversations_left("rétention conversation").await,
        (
            vec![
                "rétention conversation 23 mois".to_owned(),
                "rétention conversation en attente".to_owned(),
            ],
            4
        )
    );
    // Gone whole: the run, its sub-agent's, its event, tool call and
    // validation; what stays keeps all it had.
    assert_eq!(left_of_run(idle_run).await, 0);
    assert_eq!(left_of_run(recent_run).await, 5);
    assert_eq!(left_of_run(live_run).await, 5);

    // The entry still names the run that was; the sealed chain holds, and
    // takes in the platform's entry.
    assert_eq!(named_run(entry).await, Some(idle_run));
    intact(&verify(db, neo).await, sealed);
    seal(db, &heads, neo).await.unwrap();
    intact(&verify(db, neo).await, chain(db, neo).await.len() as i64);

    // Each company told in its own log, by the platform, of its counts alone.
    let told: Vec<_> = audit_log(&paul)
        .await
        .into_iter()
        .filter(|entry| entry["action"] == PURGED)
        .collect();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0]["actorType"], "system");
    assert_eq!(
        told[0]["metadata"],
        json!({ "conversations": 1, "messages": 2 })
    );
    assert_eq!(
        purged_entries(acme.tenant_id, PURGED).await,
        [json!({ "conversations": 1, "messages": 2 })]
    );

    // Nothing more to delete: nothing more told.
    let again = apply_retention(db).await.unwrap().unwrap();
    assert!(again.conversations.is_empty(), "{again:?}");
    assert_eq!(purged_entries(neo, PURGED).await.len(), 1);
    assert_eq!(purged_entries(acme.tenant_id, PURGED).await.len(), 1);
}
