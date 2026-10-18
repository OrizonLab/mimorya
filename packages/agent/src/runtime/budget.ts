import type { RunUsageTotals } from './state';

export interface BudgetLimits {
  maxSteps: number;
  maxInputTokens: number;
  maxOutputTokens: number;
  maxCostMicros: number;
  maxDurationMs: number;
}

/** Defaults sized for an interactive turn of a PME user. Missions raise them. */
export const DEFAULT_BUDGET: BudgetLimits = {
  maxSteps: 16,
  maxInputTokens: 600_000,
  maxOutputTokens: 60_000,
  maxCostMicros: 3_000_000, // 3 USD
  maxDurationMs: 10 * 60_000,
};

export type BudgetCheck = { ok: true } | { ok: false; reason: string };

export function checkBudget(
  limits: BudgetLimits,
  usage: RunUsageTotals,
  elapsedMs: number,
): BudgetCheck {
  if (usage.steps >= limits.maxSteps) {
    return { ok: false, reason: `nombre maximal d’étapes atteint (${limits.maxSteps})` };
  }
  if (usage.inputTokens >= limits.maxInputTokens || usage.outputTokens >= limits.maxOutputTokens) {
    return { ok: false, reason: 'budget de tokens atteint' };
  }
  if (usage.costMicros >= limits.maxCostMicros) {
    return { ok: false, reason: 'budget de coût atteint' };
  }
  if (elapsedMs >= limits.maxDurationMs) {
    return { ok: false, reason: 'durée maximale atteinte' };
  }
  return { ok: true };
}
