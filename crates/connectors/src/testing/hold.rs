//! Holds a test server's requests until the test lets them through, and
//! tells the test when the first one arrived: no timing involved. Once
//! released, it may hold the requests that follow again (`hold_again`), and
//! tell the test when one of them is held.

use tokio::sync::{Notify, watch};

pub struct Hold {
    reached: Notify,
    released: watch::Sender<bool>,
    /// The requests held right now.
    held: watch::Sender<usize>,
}

impl Default for Hold {
    fn default() -> Self {
        Self {
            reached: Notify::new(),
            released: watch::Sender::new(false),
            held: watch::Sender::new(0),
        }
    }
}

/// Counts a request as held while it waits.
struct Held<'a>(&'a watch::Sender<usize>);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.send_modify(|held| *held -= 1);
    }
}

impl Hold {
    /// Returns once the server has received a request.
    pub async fn reached(&self) {
        self.reached.notified().await;
    }

    /// Returns once a request is held: one received while not released.
    pub async fn holding(&self) {
        let mut held = self.held.subscribe();
        let _ = held.wait_for(|held| *held > 0).await;
    }

    /// Lets every held and later request through.
    pub fn release(&self) {
        self.released.send_replace(true);
    }

    /// Holds the requests received from now on, until released again.
    pub fn hold_again(&self) {
        self.released.send_replace(false);
    }

    pub(super) async fn wait(&self) {
        self.reached.notify_one();
        let mut released = self.released.subscribe();
        if *released.borrow_and_update() {
            return;
        }
        self.held.send_modify(|held| *held += 1);
        let _held = Held(&self.held);
        let _ = released.wait_for(|released| *released).await;
    }
}
