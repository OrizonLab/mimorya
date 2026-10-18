import type { Citation } from '@mimorya/core';
import type { LlmMessage, LlmToolResult } from '../llm/types';

export interface RunUsageTotals {
  steps: number;
  toolCalls: number;
  inputTokens: number;
  outputTokens: number;
  costMicros: number;
}

export interface PendingApproval {
  approvalId: string;
  toolCallId: string;
  toolName: string;
  /** Validated input, executed as-is once approved (cannot change after validation). */
  input: unknown;
}

/**
 * Everything needed to resume a run after an interruption or a human
 * validation. Serialisable as JSON (stored in `agent_runs.state`).
 */
export interface RunCheckpoint {
  version: 1;
  system: string;
  messages: LlmMessage[];
  usage: RunUsageTotals;
  correctionAttempts: number;
  evidence: Citation[];
  elapsedMs: number;
  /** Set while the run waits for human validation of some tool calls. */
  pending?: {
    completedResults: LlmToolResult[];
    approvals: PendingApproval[];
  };
}

export const ZERO_USAGE: RunUsageTotals = {
  steps: 0,
  toolCalls: 0,
  inputTokens: 0,
  outputTokens: 0,
  costMicros: 0,
};
