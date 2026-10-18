/** Public types of the agent runtime: dependencies, run configuration and outcomes. */
import type { Actor, AuthorizationEngine, AutonomyPolicy, Subject } from '@mimorya/core';
import type { ModelRouter } from '../llm/router';
import type { PriceTable } from '../llm/pricing';
import type { Effort } from '../llm/types';
import type { ToolRegistry } from '../tools/registry';
import type { BudgetLimits } from './budget';
import type { EnginePorts } from './ports';
import type { PendingApproval, RunCheckpoint } from './state';
import type { VerificationReport } from './verification';

export interface RuntimeDeps {
  router: ModelRouter;
  engine: AuthorizationEngine;
  registry: ToolRegistry;
  ports: EnginePorts;
  prices: PriceTable;
  /** Retries for retryable provider errors, on top of the SDK's own retries. */
  maxProviderRetries?: number;
  retryDelayMs?: number;
}

export interface RunConfig {
  runId: string;
  subject: Subject;
  actor: Actor;
  objective: string;
  autonomy: AutonomyPolicy;
  budget: BudgetLimits;
  successCriteria: string[];
  verification: { llmJudge: boolean; maxCorrectionAttempts: number };
  effort?: Effort;
}

export type ApprovalDecision = { approved: boolean; comment?: string; decidedBy?: string };

export type RunOutcome =
  | {
      status: 'completed';
      answer: string;
      verification: VerificationReport;
      checkpoint: RunCheckpoint;
    }
  | { status: 'awaiting_approval'; approvals: PendingApproval[]; checkpoint: RunCheckpoint }
  | { status: 'failed'; error: string; checkpoint: RunCheckpoint };
