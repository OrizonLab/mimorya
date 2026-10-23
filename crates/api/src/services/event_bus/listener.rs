//! The relay's connection to the run events channel (ADR-084). A task of
//! its own receives the notifications, never interrupted midway: when its
//! connection is lost and made again, it says so, and every followed run is
//! read again. A connection that cannot be made, or lost for good, is tried
//! again later, waiting twice as long each time up to 30 s. A read that
//! failed is tried again at a set time, whatever notices come meanwhile,
//! later each time it fails again.

use std::time::Duration;

use mimorya_db::notifications::{RUN_EVENTS_LISTENER, listen_run_events};
use sqlx::postgres::PgListener;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use super::Relay;

/// How long before trying again, doubled after each failure up to
/// `RETRY_AT_MOST`.
const RETRY: Duration = Duration::from_secs(1);
const RETRY_AT_MOST: Duration = Duration::from_secs(30);
/// Notifications received, waiting to be relayed.
const RECEIVED: usize = 256;

/// What the listener's task hands over.
enum Received {
    Notice(String),
    /// Its connection was lost and made again: notices sent meanwhile are lost.
    Reconnected,
}

impl Relay {
    /// Listens to the other processes on `url` until `stopping`, connecting
    /// again whenever the connection is lost.
    pub async fn listen(mut self, url: String, mut stopping: watch::Receiver<bool>) {
        let mut retry = RETRY;
        loop {
            let connected = tokio::select! {
                connected = listen_run_events(&url, RUN_EVENTS_LISTENER) => connected,
                _ = stopping.wait_for(|stop| *stop) => return,
            };
            match connected {
                Ok(listener) => {
                    retry = RETRY;
                    let (sender, received) = mpsc::channel(RECEIVED);
                    let receiving = tokio::spawn(receive(listener, sender));
                    let stopped = self.relay_from(received, &mut stopping).await;
                    receiving.abort();
                    self.bus.set_relaying(false);
                    if stopped {
                        return;
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        error = format!("{error:#}"),
                        "Run events listener not connected"
                    );
                }
            }
            tokio::select! {
                () = tokio::time::sleep(retry) => {}
                _ = stopping.wait_for(|stop| *stop) => return,
            }
            retry = (retry * 2).min(RETRY_AT_MOST);
        }
    }

    /// Relays what the listener's task hands over, until it stops (its
    /// connection lost for good) or the service does (`true`).
    async fn relay_from(
        &mut self,
        mut received: mpsc::Receiver<Received>,
        stopping: &mut watch::Receiver<bool>,
    ) -> bool {
        // Connected: what was sent before is read from the journals.
        self.resync().await;
        self.bus.set_relaying(true);
        let mut retry = RETRY;
        let mut read_again: Option<Instant> = None;
        loop {
            match (self.bus.relay_behind(), read_again) {
                (true, None) => {
                    read_again = Some(Instant::now() + retry);
                    retry = (retry * 2).min(RETRY_AT_MOST);
                }
                (false, _) => {
                    read_again = None;
                    retry = RETRY;
                }
                (true, Some(_)) => {}
            }
            let next = tokio::select! {
                next = received.recv() => Some(next),
                () = tokio::time::sleep_until(read_again.unwrap_or_else(Instant::now)),
                    if read_again.is_some() => None,
                _ = stopping.wait_for(|stop| *stop) => return true,
            };
            match next {
                Some(Some(Received::Notice(payload))) => self.handle(&payload).await,
                Some(Some(Received::Reconnected)) => self.resync().await,
                Some(None) => return false,
                // Time to read again what failed.
                None => {
                    read_again = None;
                    self.resync().await;
                }
            }
        }
    }
}

/// Hands over what `listener` receives, until its connection is lost for
/// good or nobody takes it any more.
async fn receive(mut listener: PgListener, sender: mpsc::Sender<Received>) {
    loop {
        let received = match listener.try_recv().await {
            Ok(Some(notification)) => Received::Notice(notification.payload().to_owned()),
            Ok(None) => Received::Reconnected,
            Err(error) => {
                tracing::warn!(error = format!("{error:#}"), "Run events listener lost");
                return;
            }
        };
        if sender.send(received).await.is_err() {
            return;
        }
    }
}
