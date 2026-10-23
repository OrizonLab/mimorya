//! The run events relay through failures (ADR-084): a read that failed is
//! tried again at a set time, whatever notices come meanwhile, and the
//! event it missed reaches the followers; a listener whose connection is
//! lost and refused is connected again later, and reads what it missed.

mod support;

use std::time::Duration;

use mimorya_core::demo::{tenant, user};
use support::TestApi;
use support::recovery::stopped_run;
use support::relay::{announce_event, next_event, record_unannounced, relaying};
use support::relay_faults::{listener_refused, reads_refused, until};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_read_that_failed_is_tried_again_whatever_notices_come_meanwhile() {
    let api = TestApi::start().await;
    relaying(&api).await;
    let bus = api.app.services.bus.clone();
    let run = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    let mut events = bus.subscribe(tenant::ID, run);
    let seq = record_unannounced(tenant::ID, run).await;

    // Its notice comes while the journal cannot be read.
    let refused = reads_refused().await;
    announce_event(run, seq).await;
    until(|| bus.relay_behind()).await;
    drop(refused);

    // Other notices keep coming: they do not put the read off.
    let noise = tokio::spawn(async {
        loop {
            announce_event(Uuid::new_v4(), 1).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    let event = next_event(&mut events, 5.0).await;
    noise.abort();
    assert_eq!(event.expect("read again").seq, Some(seq));
    assert!(!bus.relay_behind());
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_listener_refused_is_connected_again_later_and_reads_what_it_missed() {
    let api = TestApi::start().await;
    relaying(&api).await;
    let bus = api.app.services.bus.clone();
    let run = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    let mut events = bus.subscribe(tenant::ID, run);

    // Cut, and refused when it tries again: it no longer listens. Still
    // refused at its next try, a second later: it tries again after that.
    let refused = listener_refused().await;
    until(|| !bus.relaying()).await;
    let seq = record_unannounced(tenant::ID, run).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!bus.relaying());
    drop(refused);

    let event = next_event(&mut events, 10.0).await;
    assert_eq!(event.expect("read once connected again").seq, Some(seq));
    assert!(bus.relaying());
}
