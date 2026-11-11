//! What a conversation's purge does around it (ADR-186). A conversation
//! idle for twenty-four months stays while one of its runs is queued or
//! running, as when one awaits a validation (`retention_conversations.rs`).
//! One that goes takes its mission, its summary, its Slack thread and the
//! card written there; the scheduled task and the trigger that last opened
//! it stay and forget it, as the usage, memory and deliverable its run left
//! forget the run. The test starts a process without maintenance: it runs
//! the pass itself; no run is executed meanwhile.

mod support;

use mimorya_api::plugins::retention::apply_retention;
use support::conversation_links::{LinksLeft, left, link};
use support::idle_conversations::{conversation, conversations_left, run};
use support::queue::API;
use support::{TestApi, companies};

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_purged_conversation_takes_what_it_held_and_is_forgotten_by_what_named_it() {
    let api = TestApi::start_with(API).await;
    let company = companies::create(&api, "liens-retention", "admin@liens-retention.example").await;
    let (tenant, person) = (company.tenant_id, company.admin_id);

    // Idle for long, each with a run still alive: they stay.
    for status in ["queued", "running"] {
        let kept = conversation(tenant, person, &format!("liens {status}"), 26).await;
        run(tenant, kept, person, status).await;
    }
    let idle = conversation(tenant, person, "liens finie", 26).await;
    let idle_run = run(tenant, idle, person, "completed").await;
    let links = link(tenant, person, idle, idle_run).await;
    assert_eq!(
        left(&links, idle).await,
        LinksLeft {
            task: (Some(idle), Some(idle_run)),
            hook: (Some(idle), Some(idle_run)),
            gone_with_it: (1, 1, 1, 1),
            runs_named: (Some(idle_run), Some(idle_run), Some(idle_run)),
        }
    );

    let retained = apply_retention(&api.app.services.db, None)
        .await
        .unwrap()
        .unwrap();
    let lost = retained
        .conversations
        .iter()
        .find(|lost| lost.tenant_id == tenant)
        .map(|lost| (lost.conversations, lost.messages));
    assert_eq!(lost, Some((1, 2)));
    assert_eq!(
        conversations_left("liens").await.0,
        ["liens queued", "liens running"]
    );
    assert_eq!(
        left(&links, idle).await,
        LinksLeft {
            task: (None, None),
            hook: (None, None),
            gone_with_it: (0, 0, 0, 0),
            runs_named: (None, None, None),
        }
    );
}
