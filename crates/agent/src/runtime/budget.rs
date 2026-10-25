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

/// Each stage of a mission (ADR-089): more steps and time than a turn, the
/// same cost; spent, the stage hands over to the next.
pub const MISSION_STAGE_BUDGET: BudgetLimits = BudgetLimits {
    max_steps: 24,
    max_input_tokens: 800_000,
    max_output_tokens: 80_000,
    max_cost_micros: 3_000_000, // 3 USD
    max_duration_ms: 15 * 60_000,
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

/// One of `parts` shares of what `usage` and `elapsed_ms` leave of
/// `limits`, within `cap` (ADR-098): tokens and cost are split, the time
/// left is shared, the parts running side by side, and the steps are the
/// cap's own. `None` when nothing is left to share.
pub fn share_left(
    limits: &BudgetLimits,
    usage: &RunUsageTotals,
    elapsed_ms: u64,
    parts: u32,
    cap: &BudgetLimits,
) -> Option<BudgetLimits> {
    let parts = parts.max(1);
    let input = limits.max_input_tokens.saturating_sub(usage.input_tokens) / u64::from(parts);
    let output = limits.max_output_tokens.saturating_sub(usage.output_tokens) / u64::from(parts);
    let cost = (limits.max_cost_micros - usage.cost_micros) / i64::from(parts);
    let duration = limits.max_duration_ms.saturating_sub(elapsed_ms);
    if input == 0 || output == 0 || cost <= 0 || duration == 0 {
        return None;
    }
    Some(BudgetLimits {
        max_steps: cap.max_steps,
        max_input_tokens: input.min(cap.max_input_tokens),
        max_output_tokens: output.min(cap.max_output_tokens),
        max_cost_micros: cost.min(cap.max_cost_micros),
        max_duration_ms: duration.min(cap.max_duration_ms),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: BudgetLimits = BudgetLimits {
        max_steps: 8,
        max_input_tokens: 200_000,
        max_output_tokens: 20_000,
        max_cost_micros: 1_000_000,
        max_duration_ms: 300_000,
    };

    #[test]
    fn a_share_splits_what_is_left_within_the_cap() {
        let usage = RunUsageTotals {
            steps: 3,
            tool_calls: 2,
            input_tokens: 300_000,
            output_tokens: 30_000,
            cost_micros: 2_400_000,
        };
        let share = share_left(&DEFAULT_BUDGET, &usage, 9 * 60_000, 3, &CAP).unwrap();
        assert_eq!(
            share,
            BudgetLimits {
                max_steps: 8,
                max_input_tokens: 100_000,
                max_output_tokens: 10_000,
                max_cost_micros: 200_000,
                max_duration_ms: 60_000,
            }
        );
        // Plenty left: the cap holds.
        let fresh = share_left(&DEFAULT_BUDGET, &RunUsageTotals::default(), 0, 1, &CAP);
        assert_eq!(fresh, Some(CAP));
    }

    #[test]
    fn nothing_left_shares_nothing() {
        let spent = RunUsageTotals {
            cost_micros: DEFAULT_BUDGET.max_cost_micros,
            ..RunUsageTotals::default()
        };
        assert_eq!(share_left(&DEFAULT_BUDGET, &spent, 0, 1, &CAP), None);
        let late = DEFAULT_BUDGET.max_duration_ms;
        let none = RunUsageTotals::default();
        assert_eq!(share_left(&DEFAULT_BUDGET, &none, late, 1, &CAP), None);
    }
}
