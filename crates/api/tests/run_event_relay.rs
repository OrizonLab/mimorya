//! Run events across processes (ADR-084): a run executing in one process is
//! followed live from another, through PostgreSQL alone, its text included;
//! each process hands its subscribers its own events once. An event whose
//! notice was lost while nobody listened is read once the listener connects
//! again. A run's events are read in the company of whom follows it, never
//! in another. Another process's text reaches the followers, unless an
//! event they have since replaced it; a process alone sends none.
mod support;

use std::sync::Mutex;
use std::time::Duration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_core::demo::{tenant, user};
use mimorya_db::notifications::RUN_EVENTS_CHANNEL;
use mimorya_db::testing::OTHER_TENANT_ID;
use serde_json::json;
use sqlx::postgres::PgListener;
use support::TestApi;
use support::recovery::stopped_run;
use support::relay::{
    announce_delta, announce_event, cut_listeners, next_event, record_unannounced, relaying,
};
use support::runs::start_run;

/// Scripts a step that waits, once started, for the test to let it go on.
fn held_step(api: &TestApi) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
    let (started, started_seen) = std::sync::mpsc::channel::<()>();
    let (go, go_seen) = std::sync::mpsc::channel::<()>();
    let (started, go_seen) = (Mutex::new(started), Mutex::new(go_seen));
    api.script(vec![ScriptedStep::dynamic(move |_| {
        tokio::task::block_in_place(|| {
            started.lock().unwrap().send(()).unwrap();
            go_seen.lock().unwrap().recv().unwrap();
        });
        ScriptedReply::text("Deux interventions restent en cours.")
    })]);
    (started_seen, go)
}

async fn started(seen: std::sync::mpsc::Receiver<()>) {
    tokio::task::spawn_blocking(move || seen.recv().unwrap())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn a_run_executing_elsewhere_is_followed_live_from_here() {
    let executing = TestApi::start().await;
    let following = TestApi::start().await;
    let marc = executing.sign_in(user::MARC).await;
    let marc_here = following.sign_in(user::MARC).await;
    let (started_seen, go) = held_step(&executing);
    let run = start_run(&marc).await;
    started(started_seen).await;

    // Followed from a process that executes nothing: what comes once it
    // subscribed reaches it through PostgreSQL alone.
    let bus = following.app.services.bus.clone();
    let url = format!("/runs/{run}/events");
    let stream = marc_here.get(&url);
    let release = async {
        while !bus.follows(run) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        go.send(()).unwrap();
    };
    let followed = async { tokio::join!(stream, release).0 };
    let body = tokio::time::timeout(Duration::from_secs(10), followed)
        .await
        .expect("followed to its end")
        .text();
    executing.idle().await;

    assert!(body.contains("event: assistant_message"), "{body}");
    assert!(body.trim().ends_with(r#""status":"completed"}"#), "{body}");
    let ids: Vec<i32> = body
        .lines()
        .filter_map(|line| line.strip_prefix("id: "))
        .map(|id| id.parse().unwrap())
        .collect();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
    // The text, when relayed before its message, comes before it.
    if let Some(delta) = body.find("event: assistant_delta") {
        assert!(delta < body.find("event: assistant_message").unwrap());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires PostgreSQL"]
async fn this_process_s_own_events_reach_its_subscribers_once() {
    let api = TestApi::start().await;
    // Another process listens: this one sends its text too.
    let other = TestApi::start().await;
    relaying(&api).await;
    relaying(&other).await;
    let marc = api.sign_in(user::MARC).await;
    let (started_seen, go) = held_step(&api);
    let run = start_run(&marc).await;
    started(started_seen).await;
    let mut events = api.app.services.bus.subscribe(tenant::ID, run);
    go.send(()).unwrap();

    let mut seqs = Vec::new();
    let mut deltas = 0;
    // Until well after its end: a copy would come through the relay.
    while let Some(event) = next_event(&mut events, 1.0).await {
        match event.seq {
            Some(seq) => seqs.push(seq),
            None => deltas += 1,
        }
    }
    api.idle().await;
    assert!(seqs.windows(2).all(|pair| pair[0] < pair[1]), "{seqs:?}");
    assert!(!seqs.is_empty());
    assert_eq!(deltas, 1);
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn an_event_announced_while_nobody_listened_is_read_once_connected_again() {
    let api = TestApi::start().await;
    relaying(&api).await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    let mut events = api.app.services.bus.subscribe(tenant::ID, run);

    // Recorded elsewhere while this process's listener was cut.
    let seq = record_unannounced(tenant::ID, run).await;
    cut_listeners().await;
    let event = next_event(&mut events, 10.0).await.expect("read again");
    assert_eq!(
        (event.seq, event.event_type.as_str()),
        (Some(seq), "run_resumed")
    );
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_run_s_events_are_read_in_its_follower_s_company_only() {
    let api = TestApi::start().await;
    relaying(&api).await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    let seq = record_unannounced(tenant::ID, run).await;
    let witness = stopped_run(&api, tenant::ID, user::LUCAS, None, None, &[]).await;
    let witnessed = record_unannounced(tenant::ID, witness).await;
    let mut witnessing = api.app.services.bus.subscribe(tenant::ID, witness);

    // Followed in another company: its notice reads nothing there, as the
    // witness's, announced after it, shows.
    let foreign = api.app.services.bus.subscribe(OTHER_TENANT_ID, run);
    announce_event(run, seq).await;
    announce_event(witness, witnessed).await;
    let seen = next_event(&mut witnessing, 5.0)
        .await
        .expect("witness relayed");
    assert_eq!(seen.seq, Some(witnessed));
    assert!(foreign.is_empty());
    drop(foreign);

    // In its own, it is read.
    let mut own = api.app.services.bus.subscribe(tenant::ID, run);
    announce_event(run, seq).await;
    let event = next_event(&mut own, 5.0)
        .await
        .expect("read in its company");
    assert_eq!(event.seq, Some(seq));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn another_process_s_text_reaches_followers_unless_an_event_replaced_it() {
    let api = TestApi::start().await;
    relaying(&api).await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    let mut events = api.app.services.bus.subscribe(tenant::ID, run);
    let seq = record_unannounced(tenant::ID, run).await;
    announce_event(run, seq).await;
    assert_eq!(next_event(&mut events, 5.0).await.unwrap().seq, Some(seq));

    // Written before that event: stale. After it: shown.
    announce_delta(run, seq - 1, "Brouillon dépassé").await;
    announce_delta(run, seq, "Bonjour").await;
    let delta = next_event(&mut events, 5.0).await.expect("text relayed");
    assert_eq!(
        (delta.seq, delta.event_type.as_str()),
        (None, "assistant_delta")
    );
    assert_eq!(delta.data, json!({ "text": "Bonjour" }));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_text_written_elsewhere_reaches_the_followers_here() {
    let writing = TestApi::start().await;
    let following = TestApi::start().await;
    // Both listening: the writing one knows another process does.
    relaying(&writing).await;
    relaying(&following).await;
    let run = stopped_run(&following, tenant::ID, user::MARC, None, None, &[]).await;
    let mut events = following.app.services.bus.subscribe(tenant::ID, run);

    // Written until this process listens: sent while nobody did, it is lost.
    let mut relayed = None;
    for _ in 0..50 {
        writing.app.services.bus.publish_delta(run, None, "Bonjour");
        relayed = next_event(&mut events, 0.1).await;
        if relayed.is_some() {
            break;
        }
    }
    let delta = relayed.expect("text relayed");
    assert_eq!(delta.event_type, "assistant_delta");
    assert_eq!(delta.data, json!({ "text": "Bonjour" }));
}

#[tokio::test]
#[ignore = "requires PostgreSQL"]
async fn a_process_alone_sends_none_of_its_text() {
    let api = TestApi::start().await;
    relaying(&api).await;
    let run = stopped_run(&api, tenant::ID, user::MARC, None, None, &[]).await;
    // Listening as an onlooker, not as a process of the application.
    let mut onlooker = PgListener::connect(support::app_url()).await.unwrap();
    onlooker.listen(RUN_EVENTS_CHANNEL).await.unwrap();

    api.app
        .services
        .bus
        .publish_delta(run, None, "Pour personne");
    let sent = tokio::time::timeout(Duration::from_millis(500), onlooker.recv()).await;
    assert!(sent.is_err(), "{sent:?}");
}
