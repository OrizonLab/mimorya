//! Background work started by requests: agent runs, and what a request does
//! not wait for (password reset links, audits of denied logins, the lesson
//! drawn from a run). The server waits for it on shutdown, and tests wait
//! for it before asserting. A task that panics is counted out all the same:
//! nothing waits for it forever.

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
        let running = Running(self.clone());
        tokio::spawn(async move {
            // Dropped once the task ends, or once its panic unwinds it.
            let _running = running;
            task.await;
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

/// A task running, counted out when dropped.
struct Running(Arc<BackgroundTasks>);

impl Drop for Running {
    fn drop(&mut self) {
        if self.0.running.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.0.idle.notify_waiters();
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

    #[tokio::test]
    async fn a_task_that_panics_is_counted_out() {
        let tasks = Arc::new(BackgroundTasks::default());
        tasks.spawn(async { panic!("a failing task") });
        tokio::time::timeout(Duration::from_secs(5), tasks.idle())
            .await
            .expect("idle despite the panic");
    }
}
