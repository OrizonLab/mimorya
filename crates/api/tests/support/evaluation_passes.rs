//! What befalls the passes playing the evaluations (ADR-104, ADR-157):
//! from the model's step, another pass, having taken the company's lock
//! over, ends its evaluation, or another company's evaluation is played
//! meanwhile; or the session holding a company's lock is lost. The other
//! company's evaluation, asked for by an account gone, ends as soon as it
//! is played.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::skill_evaluations::evaluation_sweep;
use mimorya_core::demo::tenant;
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::OTHER_TENANT_ID;
use uuid::Uuid;

use super::leases::blocking;
use super::{app_url, owner_url};

/// Why another pass, having taken the lock over, ended an evaluation.
pub const TAKEN_OVER: &str = "Close par une autre passe.";

/// The other company's skill, whose evaluation `ask_elsewhere` asks for.
const ELSEWHERE: &str = "procedure-evaluee-ailleurs";

/// The agent answers `answer` while another pass, having taken the lock
/// over (this one's session lost), ends the company's running evaluation,
/// and one is asked for since.
pub fn taken_over(answer: &str) -> ScriptedStep {
    let answer = answer.to_owned();
    ScriptedStep::dynamic(move |_| {
        blocking(|owner| async move {
            sqlx::query(
                "update skill_evaluations set status = 'failed', failure = $1, \
                 finished_at = now() where tenant_id = $2 and status = 'running'",
            )
            .bind(TAKEN_OVER)
            .bind(tenant::ID)
            .execute(&owner)
            .await
            .unwrap();
            ask_again(&owner).await;
        });
        ScriptedReply::text(answer.clone())
    })
}

/// Ends the session holding the company `tenant_id`'s lock, as a
/// PostgreSQL restart would.
pub async fn lose_lock(tenant_id: Uuid) {
    let app = sqlx::PgPool::connect(app_url()).await.unwrap();
    let ended: Vec<bool> = sqlx::query_scalar(
        "select pg_terminate_backend(pid) from pg_stat_activity \
         where application_name = $1 and datname = current_database()",
    )
    .bind(format!("mimorya-sweep-{}", evaluation_sweep(tenant_id)))
    .fetch_all(&app)
    .await
    .unwrap();
    app.close().await;
    assert_eq!(ended, [true], "one session ended");
}

/// Another evaluation of the weekly report, asked for since.
async fn ask_again(owner: &sqlx::PgPool) {
    sqlx::query(
        "insert into skill_evaluations (tenant_id, skill_id, version, cases) \
         values ($1, $2, 1, '[]')",
    )
    .bind(tenant::ID)
    .bind(DEMO_SKILL_ID)
    .execute(owner)
    .await
    .unwrap();
}

/// The agent answers `answer` once the other company's evaluation `id` has
/// ended, ten seconds at most; `seen` gets its status then.
pub fn once_ended_elsewhere(answer: &str, id: Uuid, seen: Arc<Mutex<String>>) -> ScriptedStep {
    let answer = answer.to_owned();
    ScriptedStep::dynamic(move |_| {
        let status = blocking(move |owner| async move {
            let mut status = String::new();
            for _ in 0..100 {
                status = status_of(&owner, id).await;
                if status != "queued" && status != "running" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            status
        });
        *seen.lock().unwrap() = status;
        ScriptedReply::text(answer.clone())
    })
}

/// An evaluation of the other company's skill, asked for by an account
/// since gone.
pub async fn ask_elsewhere() -> Uuid {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let id = sqlx::query_scalar(
        "with skill as (insert into skills (tenant_id, name, description) \
         values ($1, $2, 'Une procédure d’une autre entreprise') returning id) \
         insert into skill_evaluations (tenant_id, skill_id, version, cases) \
         select $1, id, 1, '[]' from skill returning id",
    )
    .bind(OTHER_TENANT_ID)
    .bind(ELSEWHERE)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    id
}

/// The status of the other company's evaluation `id`, then its skill
/// deleted with it.
pub async fn forget_elsewhere(id: Uuid) -> String {
    let owner = sqlx::PgPool::connect(owner_url()).await.unwrap();
    let status = status_of(&owner, id).await;
    sqlx::query("delete from skills where tenant_id = $1 and name = $2")
        .bind(OTHER_TENANT_ID)
        .bind(ELSEWHERE)
        .execute(&owner)
        .await
        .unwrap();
    owner.close().await;
    status
}

async fn status_of(owner: &sqlx::PgPool, id: Uuid) -> String {
    sqlx::query_scalar("select status from skill_evaluations where id = $1")
        .bind(id)
        .fetch_one(owner)
        .await
        .unwrap()
}
