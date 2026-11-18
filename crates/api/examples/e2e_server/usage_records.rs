//! What the demo company's model calls cost, by role, written at start for
//! the consumption screen to show its breakdown (`model-escalation.spec.ts`,
//! ADR-207), the end-to-end model having no price: a call of its main
//! model, one of an escalation model and one review of an answer.

use mimorya_api::app::App;
use mimorya_core::demo::{tenant, user};

/// One call per role, and its cost in micro-dollars.
const CALLS: [(&str, &str, i64); 3] = [
    ("main", "claude-haiku-5-5", 2_500_000),
    ("escalation", "claude-opus-5-5", 1_000_000),
    ("review", "claude-sonnet-5-5", 500_000),
];

pub async fn leave(app: &App) -> anyhow::Result<()> {
    let mut tx = app.services.db.begin(tenant::ID).await?;
    for (role, model, cost_micros) in CALLS {
        sqlx::query(
            "insert into usage_records (tenant_id, user_id, kind, role, provider, model, \
             input_tokens, output_tokens, cost_micros) \
             values ($1, $2, 'llm', $3, 'anthropic', $4, 1000, 100, $5)",
        )
        .bind(tenant::ID)
        .bind(user::MARC)
        .bind(role)
        .bind(model)
        .bind(cost_micros)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
