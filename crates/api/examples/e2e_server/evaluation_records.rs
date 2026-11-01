//! What the demo company's skill evaluations left behind, written at start
//! for the screens to show it, the end-to-end model having no price: what
//! they spent today (1,234 USD, `evaluation-limit.spec.ts`, ADR-155), and
//! an evaluation of the weekly report played without the connectors that
//! reach outside the platform, as a proposal of the agent's is
//! (`skill-evaluation-note.spec.ts`, ADR-156).

use mimorya_api::app::App;
use mimorya_api::services::skill_evaluations::count_spent;
use mimorya_core::demo::{tenant, user};
use mimorya_db::seed::DEMO_SKILL_ID;

/// What the demo company's evaluations spent today, well under the
/// platform's limit.
const SPENT_MICROS: i64 = 1_234_000;

pub async fn leave(app: &App) -> anyhow::Result<()> {
    let mut tx = app.services.db.begin(tenant::ID).await?;
    count_spent(&mut tx, tenant::ID, SPENT_MICROS).await?;
    sqlx::query(
        "insert into skill_evaluations (tenant_id, skill_id, version, launched_by_user_id, \
         cases, status, started_at, finished_at, inside_only) \
         values ($1, $2, 1, $3, '[]', 'completed', now(), now(), true)",
    )
    .bind(tenant::ID)
    .bind(DEMO_SKILL_ID)
    .bind(user::PAUL)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
