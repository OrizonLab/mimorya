/**
 * Autonomy levels decide what the agent may do with a tool once RBAC/ABAC has
 * allowed it. Authorization answers "may this user do it?"; autonomy answers
 * "may the agent do it on its own, or must a human validate first?".
 */

/** Risk class declared by every tool. Ordered from harmless to irreversible. */
export const TOOL_RISKS = ['read', 'draft', 'write', 'external', 'irreversible'] as const;
export type ToolRisk = (typeof TOOL_RISKS)[number];
// read         — retrieval without side effects
// draft        — creates internal drafts/proposals, no effect outside the platform
// write        — modifies data in a connected system (reversible)
// external     — sends something outside the company (email, client message…)
// irreversible — deletion, payment, anything that cannot be undone

export const AUTONOMY_LEVELS = [
  'read_only',
  'propose',
  'execute_with_approval',
  'autonomous_limited',
] as const;
export type AutonomyLevel = (typeof AUTONOMY_LEVELS)[number];

export interface AutonomyPolicy {
  level: AutonomyLevel;
  /** Tools explicitly allowed to run without approval at `autonomous_limited`. */
  autonomousTools?: ReadonlySet<string>;
}

export type ExecutionDecision =
  | { kind: 'allow' }
  | { kind: 'require_approval'; reason: string }
  | { kind: 'deny'; reason: string };

export interface ToolRiskProfile {
  name: string;
  risk: ToolRisk;
  /** Forces human validation whatever the autonomy level. */
  requiresApproval?: boolean;
}

export function decideExecution(policy: AutonomyPolicy, tool: ToolRiskProfile): ExecutionDecision {
  const sideEffect = tool.risk !== 'read' && tool.risk !== 'draft';

  if (policy.level === 'read_only' && tool.risk !== 'read') {
    return { kind: 'deny', reason: 'Niveau d’autonomie « lecture seule »' };
  }
  if (policy.level === 'propose' && sideEffect) {
    return {
      kind: 'deny',
      reason: 'Niveau d’autonomie « proposition » : l’action doit être proposée, pas exécutée',
    };
  }
  if (tool.requiresApproval) {
    return { kind: 'require_approval', reason: 'Cet outil exige une validation humaine' };
  }
  if (!sideEffect) return { kind: 'allow' };

  if (tool.risk === 'irreversible') {
    return { kind: 'require_approval', reason: 'Action irréversible' };
  }
  if (policy.level === 'autonomous_limited' && policy.autonomousTools?.has(tool.name)) {
    return { kind: 'allow' };
  }
  return {
    kind: 'require_approval',
    reason: tool.risk === 'external' ? 'Action à impact externe' : 'Action modifiant des données',
  };
}
