//! Sub-agents (ADR-098): a run hands precise tasks to child runs through
//! `delegate_task`. Each child is recorded beside its parent with a
//! profile, and works with the tools that only read and run without
//! validation, its parent's permission ceiling, the read-only autonomy and
//! a share of what is left of its parent's budget; it delegates nothing
//! further. It returns a summary, with the sources it read, which its
//! parent may cite; what it spent counts in its parent's budget. At most
//! `MAX_PER_TURN` calls of a turn run, side by side.

mod child;
mod profile;
mod prompt;
mod store;

pub use child::{SubAgentReport, SubAgents};
pub use profile::SubAgentProfile;
pub use prompt::{sub_agent_system, sub_agent_turn};
pub use store::{SubRun, SubRunRequest, SubRunStore};

use super::budget::BudgetLimits;

/// Delegations of one turn run side by side, at most.
pub const MAX_PER_TURN: usize = 3;

/// Why a delegation beyond `MAX_PER_TURN` in a turn does not run.
pub const TOO_MANY_AT_ONCE: &str =
    "Au plus trois sous-agents par étape : confiez cette tâche à l’étape suivante.";

/// What one sub-agent may spend at most, within its share of its parent's
/// budget.
pub const SUB_AGENT_BUDGET: BudgetLimits = BudgetLimits {
    max_steps: 8,
    max_input_tokens: 200_000,
    max_output_tokens: 20_000,
    max_cost_micros: 1_000_000, // 1 USD
    max_duration_ms: 5 * 60_000,
};
