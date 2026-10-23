//! Faults the run events relay recovers from (ADR-084): its reads refused
//! for a while, its listener's connection cut and refused. Each is undone
//! when its guard goes, the test failing or not.

use std::time::Duration;

use super::{app_url, owner_url};

/// Undoes a fault, as the owner, when dropped.
pub struct Undo(String);

impl Drop for Undo {
    fn drop(&mut self) {
        let statement = std::mem::take(&mut self.0);
        // From a thread of its own: dropped within the test's runtime.
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(as_owner(&statement));
        })
        .join()
        .unwrap();
    }
}

async fn as_owner(statement: &str) {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    sqlx::query(statement).execute(&owner).await.unwrap();
    owner.close().await;
}

/// Refuses the application's role the reading of run events.
pub async fn reads_refused() -> Undo {
    as_owner("revoke select on run_events from mimorya_app").await;
    Undo("grant select on run_events to mimorya_app".into())
}

/// Refuses the application's role any new connection, then cuts the
/// relays' listeners (with a connection of that role opened beforehand).
pub async fn listener_refused() -> Undo {
    let app = sqlx::PgPool::connect(app_url()).await.unwrap();
    let database: String = sqlx::query_scalar("select current_database()")
        .fetch_one(&app)
        .await
        .unwrap();
    as_owner(&format!(
        "revoke connect on database \"{database}\" from public, mimorya_app"
    ))
    .await;
    let undo = Undo(format!(
        "grant connect on database \"{database}\" to public, mimorya_app"
    ));
    sqlx::query(
        "select pg_terminate_backend(pid) from pg_stat_activity \
         where application_name = 'mimorya-run-events' and datname = current_database()",
    )
    .execute(&app)
    .await
    .unwrap();
    app.close().await;
    undo
}

/// Waits until `condition` holds, for five seconds at most.
pub async fn until(condition: impl Fn() -> bool) {
    for _ in 0..250 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("not reached in time");
}
