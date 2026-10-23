//! The run queue in the background (ADR-085): once the runs a stop
//! interrupted are taken up, the process that executes runs claims the
//! queued ones, woken up as soon as one is queued, and every few seconds
//! should a notice be lost; until the service stops, between two runs.

use std::sync::Arc;
use std::time::Duration;

use mimorya_db::notifications::listen_run_queue;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::app::AppState;

/// How often the queue is read, notices or not.
const POLL: Duration = Duration::from_secs(5);
/// How long before listening again after a failure.
const RETRY: Duration = Duration::from_secs(5);

pub fn spawn_run_queue(services: AppState) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut stopping = services.shutdown.subscribe();
        tokio::select! {
            () = services.runs.interrupted_runs_taken_up() => {}
            _ = stopping.wait_for(|stop| *stop) => return,
        }
        let queued = Arc::new(Notify::new());
        let _listening = Listening(tokio::spawn(wake_on_notices(
            services.config.database_app_url.clone(),
            queued.clone(),
        )));
        loop {
            match services.runs.take_queued().await {
                Ok(0) => {}
                Ok(taken) => tracing::info!(taken, "Queued runs taken"),
                Err(error) => tracing::warn!(error = format!("{error:#}"), "Run queue not read"),
            }
            tokio::select! {
                () = queued.notified() => {}
                () = tokio::time::sleep(POLL) => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
        }
    })
}

/// The listening task, stopped with the queue's.
struct Listening(JoinHandle<()>);

impl Drop for Listening {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Wakes the queue up at each notice that a run is queued, listening again
/// after a failure: the queue is read every few seconds meanwhile.
async fn wake_on_notices(url: String, queued: Arc<Notify>) {
    loop {
        match listen_run_queue(&url).await {
            Ok(mut listener) => loop {
                match listener.recv().await {
                    Ok(_) => queued.notify_one(),
                    Err(error) => {
                        tracing::warn!(error = format!("{error:#}"), "Run queue listener lost");
                        break;
                    }
                }
            },
            Err(error) => {
                tracing::warn!(
                    error = format!("{error:#}"),
                    "Run queue listener not connected"
                );
            }
        }
        tokio::time::sleep(RETRY).await;
    }
}
