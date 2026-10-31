//! What a call the platform makes on its own once a run ends costs (a lesson
//! drawn from it, ADR-076; its conversation summarised, ADR-145): counted in
//! that run, as its own calls are.

use mimorya_agent::llm::{LlmUsage, PriceTable};
use mimorya_db::Database;
use mimorya_db::repositories::usage::{NewUsage, append_usage};

use super::db_ports::RunScope;

/// The call `model` of `provider` answered, at the cost of `usage`.
pub struct BackgroundCall<'a> {
    pub provider: &'a str,
    pub model: &'a str,
    pub usage: &'a LlmUsage,
}

pub async fn record_background_usage(
    db: &Database,
    prices: &PriceTable,
    scope: RunScope,
    call: BackgroundCall<'_>,
) -> anyhow::Result<()> {
    let usage = call.usage;
    let mut tx = db.begin(scope.tenant_id).await?;
    append_usage(
        &mut tx,
        NewUsage {
            tenant_id: scope.tenant_id,
            user_id: Some(scope.user_id),
            run_id: Some(scope.run_id),
            kind: "llm",
            provider: call.provider.to_owned(),
            model: call.model.to_owned(),
            // Counted as runs count them: cached tokens are input too.
            input_tokens: i32::try_from(usage.input_total()).unwrap_or(i32::MAX),
            output_tokens: i32::try_from(usage.output_tokens).unwrap_or(i32::MAX),
            cost_micros: prices.cost_micros(call.model, usage).unwrap_or(0),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
