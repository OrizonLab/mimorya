//! The bus: subscribers here, what they have, deltas on their way out.

use serde_json::json;

use super::*;

fn event(seq: i32) -> BusEvent {
    BusEvent {
        seq: Some(seq),
        event_type: "tool_call".into(),
        data: json!({}),
    }
}

#[tokio::test]
async fn delivers_events_of_the_subscribed_run_only() {
    let bus = Arc::new(RunEventBus::default());
    let (tenant, run) = (Uuid::new_v4(), Uuid::new_v4());
    let mut subscription = bus.subscribe(tenant, run);
    assert!(bus.follows(run));
    bus.publish(Uuid::new_v4(), event(9));
    bus.publish(run, event(1));
    assert_eq!(subscription.recv().await.unwrap().seq, Some(1));
    assert!(subscription.is_empty());
    drop(subscription);
    assert!(!bus.follows(run));
    assert!(bus.listeners.lock().unwrap().is_empty());
}

#[test]
fn what_a_run_s_subscribers_have_goes_with_the_last() {
    let bus = Arc::new(RunEventBus::default());
    let (tenant, run) = (Uuid::new_v4(), Uuid::new_v4());
    let first = bus.subscribe(tenant, run);
    let handed = |bus: &RunEventBus| bus.followed_run(run).unwrap().handed;
    assert_eq!(handed(&bus), None);
    // Replayed: from there; less by another: from there.
    first.replayed_through(5);
    let second = bus.subscribe(tenant, run);
    second.replayed_through(3);
    assert_eq!(handed(&bus), Some(3));
    // Published since: all have it.
    bus.publish(run, event(7));
    assert_eq!(
        bus.followed(),
        [Followed {
            tenant_id: tenant,
            run_id: run,
            handed: Some(7)
        }]
    );
    drop(first);
    drop(second);
    assert_eq!(bus.followed_run(run), None);
    assert!(bus.followed().is_empty());
}

#[tokio::test]
async fn a_delta_goes_to_subscribers_here_and_to_the_other_processes() {
    let (bus, mut outbox) = RunEventBus::new();
    let bus = Arc::new(bus);
    let (tenant, run) = (Uuid::new_v4(), Uuid::new_v4());
    let mut subscription = bus.subscribe(tenant, run);
    bus.publish_delta(run, Some(4), "Bonjour");
    let here = subscription.recv().await.unwrap();
    assert_eq!(
        (here.seq, here.event_type.as_str()),
        (None, "assistant_delta")
    );
    assert_eq!(here.data, json!({ "text": "Bonjour" }));
    let sent = outbox.next_batch().await.unwrap();
    assert_eq!(
        (sent.run_id, sent.after, sent.text.as_str()),
        (run, Some(4), "Bonjour")
    );
}
