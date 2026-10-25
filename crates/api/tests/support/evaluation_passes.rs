//! What befalls the pass playing the evaluations (ADR-104) while a case is
//! played, from the model's step: another pass, having taken the lock
//! over, ends the evaluation; or the session holding the lock is lost
//! while another company asks for one.

use mimorya_agent::testing::{ScriptedReply, ScriptedStep};
use mimorya_api::services::skill_evaluations::EVALUATION_SWEEP;
use mimorya_core::demo::tenant;
use mimorya_db::seed::DEMO_SKILL_ID;
use mimorya_db::testing::OTHER_TENANT_ID;

use super::app_url;
use super::leases::blocking;

/// Why another pass, having taken the lock over, ended an evaluation.
pub const TAKEN_OVER: &str = "Close par une autre passe.";

/// The other company's skill, whose evaluation `lock_lost` asks for.
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
            sqlx::query(
                "insert into skill_evaluations (tenant_id, skill_id, version, cases) \
                 values ($1, $2, 1, '[]')",
            )
            .bind(tenant::ID)
            .bind(DEMO_SKILL_ID)
            .execute(&owner)
            .await
            .unwrap();
        });
        ScriptedReply::text(answer.clone())
    })
}

/// The agent answers `answer` while the session holding the pass's lock is
/// lost (PostgreSQL restarted), and another company asks for an evaluation.
pub fn lock_lost(answer: &str) -> ScriptedStep {
    let answer = answer.to_owned();
    ScriptedStep::dynamic(move |_| {
        blocking(|owner| async move {
            let app = sqlx::PgPool::connect(app_url()).await.unwrap();
            sqlx::query(
                "select pg_terminate_backend(pid) from pg_stat_activity \
                 where application_name = $1 and datname = current_database()",
            )
            .bind(format!("mimorya-sweep-{EVALUATION_SWEEP}"))
            .execute(&app)
            .await
            .unwrap();
            app.close().await;
            sqlx::query(
                "with skill as (insert into skills (tenant_id, name, description) \
                 values ($1, $2, 'Une procédure d’une autre entreprise') returning id) \
                 insert into skill_evaluations (tenant_id, skill_id, version, cases) \
                 select $1, id, 1, '[]' from skill",
            )
            .bind(OTHER_TENANT_ID)
            .bind(ELSEWHERE)
            .execute(&owner)
            .await
            .unwrap();
        });
        ScriptedReply::text(answer.clone())
    })
}

/// The status of the evaluation `lock_lost` asked for, then deleted with
/// its skill.
pub fn forget_elsewhere() -> String {
    blocking(|owner| async move {
        let status = sqlx::query_scalar(
            "select e.status from skill_evaluations e join skills s on s.id = e.skill_id \
             where s.tenant_id = $1 and s.name = $2",
        )
        .bind(OTHER_TENANT_ID)
        .bind(ELSEWHERE)
        .fetch_one(&owner)
        .await
        .unwrap();
        sqlx::query("delete from skills where tenant_id = $1 and name = $2")
            .bind(OTHER_TENANT_ID)
            .bind(ELSEWHERE)
            .execute(&owner)
            .await
            .unwrap();
        status
    })
}
