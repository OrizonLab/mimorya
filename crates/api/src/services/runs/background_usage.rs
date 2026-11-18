//! What a call the platform makes on its own once a run ends costs (a lesson
//! drawn from it, ADR-076; its conversation summarised, ADR-145): counted in
//! that run, as its own calls are, on its runs' model and key (ADR-208).

use mimorya_agent::llm::{LlmUsage, ModelRole, PriceTable};
use mimorya_db::Database;
use mimorya_db::repositories::usage::{NewUsage, append_usage};

use super::db_ports::RunScope;
use crate::services::llm::Models;

/// The call `model` of `provider` answered, at the cost of `usage`.
pub struct BackgroundCall<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub usage: &'a LlmUsage,
}

/// Records the call, charged by the `models` it was made with (ADR-208).
pub async fn record_background_usage(
    db: &Database,
    prices: &PriceTable,
    models: &Models,
    scope: RunScope,
    call: BackgroundCall<'_>,
) -> anyhow::Result<()> {
    let usage = call.usage;
    let cost_micros = prices.cost_micros(call.model, usage).unwrap_or(0);
    let charge = models.charge(ModelRole::Main, cost_micros);
    let mut tx = db.begin(scope.tenant_id).await?;
    append_usage(
        &mut tx,
        NewUsage {
            tenant_id: scope.tenant_id,
            user_id: Some(scope.user_id),
            run_id: Some(scope.run_id),
            kind: "llm",
            role: ModelRole::Main.as_str(),
            provider: call.provider.to_owned(),
            model: call.model.to_owned(),
            // Counted as runs count them: cached tokens are input too.
            input_tokens: i32::try_from(usage.input_total()).unwrap_or(i32::MAX),
            output_tokens: i32::try_from(usage.output_tokens).unwrap_or(i32::MAX),
            cost_micros,
            charge,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
