//! A run failed by another process's sweep while its model still answers,
//! as a stuck run would be (the stale-run sweep, ADR-016, ADR-053).

use mimorya_api::plugins::maintenance::sweep_stale_runs;
use mimorya_api::services::RunEventBus;
use mimorya_db::{Database, PoolOptions};
use uuid::Uuid;

use super::{app_url, owner_url};

/// Ages the user's running runs and sweeps them from another thread, with
/// its own connection and bus: as another process would, while the model
/// still answers.
pub fn sweep_from_elsewhere(user_id: Uuid) {
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(async {
                let owner = sqlx::PgPool::connect(owner_url()).await.expect("owner");
                sqlx::query(
                    "update agent_runs set updated_at = now() - interval '20 minutes' \
                     where status = 'running' and on_behalf_of_user_id = $1",
                )
                .bind(user_id)
                .execute(&owner)
                .await
                .expect("aged");
                let db = Database::connect(
                    app_url(),
                    PoolOptions {
                        max_connections: 1,
                        application_name: "other-process",
                    },
                )
                .await
                .expect("app connection");
                let bus = RunEventBus::default();
                sweep_stale_runs(&db, &bus, chrono::Duration::minutes(15))
                    .await
                    .expect("sweep");
            });
    })
    .join()
    .expect("sweep thread");
}
