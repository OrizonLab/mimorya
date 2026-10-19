//! Holds a test server's requests until the test lets them through, and
//! tells the test when the first one arrived: no timing involved.

use tokio::sync::{Notify, watch};

pub struct Hold {
    reached: Notify,
    released: watch::Sender<bool>,
}

impl Default for Hold {
    fn default() -> Self {
        Self {
            reached: Notify::new(),
            released: watch::Sender::new(false),
        }
    }
}

impl Hold {
    /// Returns once the server has received a request.
    pub async fn reached(&self) {
        self.reached.notified().await;
    }

    /// Lets every held and later request through.
    pub fn release(&self) {
        self.released.send_replace(true);
    }

    pub(super) async fn wait(&self) {
        self.reached.notify_one();
        let mut released = self.released.subscribe();
        let _ = released.wait_for(|released| *released).await;
    }
}
