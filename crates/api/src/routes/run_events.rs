//! Server-sent events for a run. Persisted events are replayed after
//! `Last-Event-ID` (or `?after=`), then live events follow. The stream ends
//! when the run reaches a terminal state, when the client leaves, or when the
//! server stops.

use std::convert::Infallible;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::header::{CACHE_CONTROL, HeaderName};
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use mimorya_core::DomainError;
use mimorya_db::repositories::run_events::{RunEventRow, list_run_events};
use mimorya_db::repositories::runs::{RunStatus, get_run};
use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::ReceiverStream;
use validator::Validate;

use crate::app::AppState;
use crate::http::{ApiResult, PathId, Query};
use crate::plugins::Auth;
use crate::services::BusEvent;
use crate::services::event_bus::Subscription;

const HEARTBEAT: Duration = Duration::from_secs(15);
/// How long a stream on an already finished run waits for its last events.
const FINISH_GRACE: Duration = Duration::from_secs(5);

type SseSender = mpsc::Sender<Result<Event, Infallible>>;

pub fn routes() -> Router<AppState> {
    Router::new().route("/runs/{id}/events", get(stream))
}

#[derive(Deserialize, Validate)]
struct EventsQuery {
    #[validate(range(min = 0))]
    after: Option<i32>,
}

async fn stream(
    State(state): State<AppState>,
    Auth(identity): Auth,
    PathId(id): PathId,
    headers: HeaderMap,
    Query(query): Query<EventsQuery>,
) -> ApiResult<impl IntoResponse> {
    let last_event_id = match headers.get("last-event-id") {
        Some(value) => value.to_str().ok().and_then(|v| v.trim().parse().ok()),
        None => query.after,
    }
    .unwrap_or(0)
    .max(0);

    let mut tx = state.db.begin(identity.tenant_id).await?;
    get_run(&mut tx, id)
        .await?
        .filter(|r| r.on_behalf_of_user_id == identity.user_id)
        .ok_or_else(|| DomainError::not_found("Exécution introuvable"))?;
    tx.commit().await?;

    // Subscribe before replaying so that nothing is lost in between.
    let subscription = state.bus.subscribe(identity.tenant_id, id);
    let mut tx = state.db.begin(identity.tenant_id).await?;
    let persisted = list_run_events(&mut tx, id, last_event_id).await?;
    let finished = get_run(&mut tx, id)
        .await?
        .is_some_and(|r| r.status().is_finished());
    tx.commit().await?;
    // What another process records next is relayed from there (ADR-084):
    // what was replayed, not what the client says it has.
    if let Some(last) = persisted.last() {
        subscription.replayed_through(last.seq);
    }

    let (sender, receiver) = mpsc::channel(64);
    // Nothing more will come for a run that already ended.
    if !(finished && persisted.is_empty() && subscription.is_empty()) {
        let replay = Replay {
            persisted,
            subscription,
            last_seq: last_event_id,
            finished,
            shutdown: state.shutdown.subscribe(),
        };
        tokio::spawn(replay.forward(sender));
    }
    let sse = Sse::new(ReceiverStream::new(receiver))
        .keep_alive(KeepAlive::new().interval(HEARTBEAT).text("keep-alive"));
    Ok((
        [
            (CACHE_CONTROL, "no-cache, no-transform"),
            (HeaderName::from_static("x-accel-buffering"), "no"),
        ],
        sse,
    ))
}

struct Replay {
    persisted: Vec<RunEventRow>,
    subscription: Subscription,
    last_seq: i32,
    /// The run had already ended when the stream opened.
    finished: bool,
    shutdown: watch::Receiver<bool>,
}

impl Replay {
    async fn forward(mut self, sender: SseSender) {
        for row in std::mem::take(&mut self.persisted) {
            let event = BusEvent {
                seq: Some(row.seq),
                event_type: row.event_type,
                data: row.data,
            };
            if !self.deliver(&sender, event).await {
                return;
            }
        }
        while !*self.shutdown.borrow() {
            let next = tokio::select! {
                event = self.subscription.recv() => event,
                _ = sender.closed() => None,
                _ = self.shutdown.changed() => None,
                _ = tokio::time::sleep(FINISH_GRACE), if self.finished => None,
            };
            let Some(event) = next else { return };
            if !self.deliver(&sender, event).await {
                return;
            }
        }
    }

    /// Sends one event; `false` when the stream must end.
    async fn deliver(&mut self, sender: &SseSender, event: BusEvent) -> bool {
        let mut sse = Event::default();
        if let Some(seq) = event.seq {
            if seq <= self.last_seq {
                return true;
            }
            self.last_seq = seq;
            sse = sse.id(seq.to_string());
        }
        let sse = sse.event(&event.event_type).data(event.data.to_string());
        if sender.send(Ok(sse)).await.is_err() {
            return false;
        }
        !is_final(&event)
    }
}

fn is_final(event: &BusEvent) -> bool {
    event.event_type == "run_finished"
        && event.data["status"]
            .as_str()
            .and_then(RunStatus::parse)
            .is_some_and(RunStatus::is_finished)
}
