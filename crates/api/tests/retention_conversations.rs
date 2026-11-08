//! Retention applied to the conversations (DATA_MODEL §5, ADR-186). A pass
//! deletes the conversations idle for twenty-four months, whole: their
//! messages, runs and sub-agents' runs, events, tool calls and validations.
//! A conversation more recent, or with a run still awaiting a validation,
//! stays. Each company that lost conversations finds it in its own audit
//! log, by the platform, counted, never read; a pass with nothing to delete
//! tells no one. An audit entry naming a deleted run keeps naming it, and
//! the company's sealed chain holds. They go a batch at a time, the oldest
//! first: a backlog over several statements, each with its own entry. The
//! tests start a process without maintenance: they run the passes and the
//! sealing themselves.

mod support;

use mimorya_api::plugins::retention::{CONVERSATION_BATCH, apply_retention};
use mimorya_api::services::audit_chain::SealedHeads;
use mimorya_core::demo::{tenant, user};
use mimorya_db::repositories::retention::{PurgedConversations, purge_idle_conversations};
use serde_json::json;
use support::audit_chain::{chain, intact, seal, verify};
use support::audit_exports::audit_log;
use support::idle_conversations::{
    audited_run, conversation, conversations_left, idle_backlog, left_of_run, named_run, run,
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

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn idle_conversations_go_a_batch_at_a_time_the_oldest_first() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let company = companies::create(
        &api,
        "lots-conversations",
        "admin@lots-conversations.example",
    )
    .await;
    for months in [40, 41, 42] {
        let title = format!("rétention lot {months}");
        conversation(company.tenant_id, company.admin_id, &title, months).await;
    }
    let lost = |conversations, messages| {
        vec![PurgedConversations {
            tenant_id: company.tenant_id,
            conversations,
            messages,
        }]
    };

    // A batch is one conversation at least: the oldest.
    assert_eq!(
        purge_idle_conversations(db.pool(), 0).await.unwrap(),
        lost(1, 2)
    );
    assert_eq!(
        conversations_left("rétention lot").await.0,
        ["rétention lot 40", "rétention lot 41"]
    );
    assert_eq!(
        purge_idle_conversations(db.pool(), 2).await.unwrap(),
        lost(2, 4)
    );
    assert_eq!(conversations_left("rétention lot").await, (vec![], 0));
    // Each statement told the company of its own batch.
    assert_eq!(
        purged_entries(company.tenant_id, PURGED).await,
        [
            json!({ "conversations": 1, "messages": 2 }),
            json!({ "conversations": 2, "messages": 4 }),
        ]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_backlog_goes_over_several_statements_of_one_pass() {
    let api = TestApi::start_with(API).await;
    let company = companies::create(&api, "arriere-conversations", "admin@arriere.example").await;
    let backlog = CONVERSATION_BATCH + 1;
    idle_backlog(
        company.tenant_id,
        company.admin_id,
        "rétention arriéré",
        backlog,
        30,
    )
    .await;

    let retained = apply_retention(&api.app.services.db)
        .await
        .unwrap()
        .unwrap();
    let lost = retained
        .conversations
        .iter()
        .find(|lost| lost.tenant_id == company.tenant_id)
        .map(|lost| (lost.conversations, lost.messages));
    assert_eq!(lost, Some((backlog, 0)));
    assert_eq!(conversations_left("rétention arriéré").await, (vec![], 0));
    assert_eq!(
        purged_entries(company.tenant_id, PURGED).await,
        [
            json!({ "conversations": CONVERSATION_BATCH, "messages": 0 }),
            json!({ "conversations": 1, "messages": 0 }),
        ]
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_batch_is_five_hundred_conversations_at_most() {
    let api = TestApi::start_with(API).await;
    let db = &api.app.services.db;
    let company = companies::create(&api, "borne-conversations", "admin@borne.example").await;
    // Older than any other test's: the batch takes them first.
    idle_backlog(
        company.tenant_id,
        company.admin_id,
        "rétention borne",
        501,
        60,
    )
    .await;

    let batch = purge_idle_conversations(db.pool(), 10_000).await.unwrap();
    let lost = batch
        .iter()
        .find(|lost| lost.tenant_id == company.tenant_id)
        .map(|lost| lost.conversations);
    assert_eq!(lost, Some(500));
    assert_eq!(conversations_left("rétention borne").await.0.len(), 1);

    // The last one goes with the next batch: nothing idle is left for the
    // other tests.
    let next = purge_idle_conversations(db.pool(), CONVERSATION_BATCH)
        .await
        .unwrap();
    let lost = next
        .iter()
        .find(|lost| lost.tenant_id == company.tenant_id)
        .map(|lost| lost.conversations);
    assert_eq!(lost, Some(1));
    assert_eq!(conversations_left("rétention borne").await, (vec![], 0));
}
