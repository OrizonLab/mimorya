//! Run budgets: steps, tokens, cost and duration.

use serde::{Deserialize, Serialize};

use super::state::RunUsageTotals;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetLimits {
    pub max_steps: u32,
    pub max_input_tokens: u64,
    pub max_output_tokens: u64,
    pub max_cost_micros: i64,
    pub max_duration_ms: u64,
}

/// Defaults sized for an interactive turn of a PME user. Missions raise them.
pub const DEFAULT_BUDGET: BudgetLimits = BudgetLimits {
    max_steps: 16,
    max_input_tokens: 600_000,
    max_output_tokens: 60_000,
    max_cost_micros: 3_000_000, // 3 USD
    max_duration_ms: 10 * 60_000,
};

/// `Err` carries the reason the run must stop.
pub fn check_budget(
    limits: &BudgetLimits,
    usage: &RunUsageTotals,
    elapsed_ms: u64,
) -> Result<(), String> {
    if usage.steps >= limits.max_steps {
        return Err(format!(
            "nombre maximal d’étapes atteint ({})",
            limits.max_steps
        ));
    }
    if usage.input_tokens >= limits.max_input_tokens
        || usage.output_tokens >= limits.max_output_tokens
    {
        return Err("budget de tokens atteint".into());
    }
    if usage.cost_micros >= limits.max_cost_micros {
        return Err("budget de coût atteint".into());
    }
    if elapsed_ms >= limits.max_duration_ms {
        return Err("durée maximale atteinte".into());
    }
    Ok(())
}
