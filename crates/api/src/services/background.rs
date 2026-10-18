//! Background work started by requests (agent runs). The server waits for it
//! on shutdown, and tests wait for it before asserting.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use tokio::sync::Notify;

#[derive(Default)]
pub struct BackgroundTasks {
    running: AtomicUsize,
    idle: Notify,
}

impl BackgroundTasks {
    pub fn spawn<F>(self: &Arc<Self>, task: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.running.fetch_add(1, Ordering::SeqCst);
        let tasks = self.clone();
        tokio::spawn(async move {
            task.await;
            if tasks.running.fetch_sub(1, Ordering::SeqCst) == 1 {
                tasks.idle.notify_waiters();
            }
        });
    }

    /// Resolves when no task is running.
    pub async fn idle(&self) {
        loop {
            let notified = self.idle.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.running.load(Ordering::SeqCst) == 0 {
                return;
            }
            notified.await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn waits_for_every_task() {
        let tasks = Arc::new(BackgroundTasks::default());
        let done = Arc::new(AtomicUsize::new(0));
        for delay in [10, 30] {
            let done = done.clone();
            tasks.spawn(async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
                done.fetch_add(1, Ordering::SeqCst);
            });
        }
        tasks.idle().await;
        assert_eq!(done.load(Ordering::SeqCst), 2);
    }
}
