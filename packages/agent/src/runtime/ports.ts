/**
 * Ports the engine depends on. `packages/db` provides PostgreSQL
 * implementations (wired in apps/api); `testing/` provides in-memory ones.
 */
import type {
  Citation,
  DeliverableService,
  MemoryKind,
  MemoryReadFilter,
  MemoryScope,
  Sensitivity,
  ToolRisk,
} from '@mimorya/core';
import type { RunCheckpoint } from './state';

export type RunEvent =
  | { type: 'run_started'; objective: string }
  | { type: 'assistant_delta'; text: string }
  | { type: 'assistant_message'; text: string; final: boolean }
  | {
      type: 'tool_call';
      toolCallId: string;
      toolName: string;
      title: string;
      decision: 'allow' | 'require_approval' | 'deny';
      reason?: string;
    }
  | {
      type: 'tool_result';
      toolCallId: string;
      toolName: string;
      summary: string;
      isError: boolean;
      durationMs: number;
    }
  | {
      type: 'approval_requested';
      approvalId: string;
      toolName: string;
      actionSummary: string;
      risk: ToolRisk;
    }
  | { type: 'approval_resolved'; approvalId: string; approved: boolean }
  | { type: 'verification'; attempt: number; passed: boolean; issues: string[] }
  | { type: 'usage'; inputTokens: number; outputTokens: number; costMicros: number | null }
  | {
      type: 'run_finished';
      status: 'completed' | 'awaiting_approval' | 'failed' | 'cancelled';
      error?: string;
    };

/** Persists and broadcasts run events. Transient events (deltas) may be broadcast only. */
export interface RunEventSink {
  emit(event: RunEvent): Promise<void>;
}

export interface AuditEntry {
  action: string;
  outcome: 'success' | 'denied' | 'failure';
  resourceType?: string;
  resourceId?: string;
  reason?: string;
  metadata?: Record<string, unknown>;
}

/** Audit entries recorded by the engine are attributed to "agent on behalf of user". */
export interface AuditSink {
  record(entry: AuditEntry): Promise<void>;
}

export interface ToolInvocationRecord {
  toolCallId: string;
  toolName: string;
  input: unknown;
  decision: 'allow' | 'require_approval' | 'deny';
  decisionReason?: string;
  status: 'succeeded' | 'failed' | 'denied' | 'awaiting_approval' | 'rejected';
  outputSummary?: string;
  error?: string;
  durationMs?: number;
  approvalId?: string;
}

export interface ToolInvocationLog {
  record(entry: ToolInvocationRecord): Promise<void>;
}

export interface ApprovalRequest {
  toolName: string;
  toolCallId: string;
  actionSummary: string;
  payload: unknown;
  risk: ToolRisk;
  reason: string;
}

export interface ApprovalStore {
  create(request: ApprovalRequest): Promise<{ id: string }>;
}

export interface UsageRecord {
  kind: 'llm' | 'embedding' | 'tool';
  provider: string;
  model: string;
  inputTokens: number;
  outputTokens: number;
  costMicros: number;
}

export interface UsageSink {
  record(usage: UsageRecord): Promise<void>;
}

export interface MemoryHit {
  id: string;
  scope: MemoryScope;
  kind: MemoryKind;
  title: string;
  content: string;
  orgUnitId: string | null;
  updatedAt: Date;
  score: number;
}

export interface NewMemory {
  scope: MemoryScope;
  ownerUserId?: string | null;
  orgUnitId?: string | null;
  kind: MemoryKind;
  title: string;
  content: string;
  sensitivity?: Sensitivity;
  status: 'active' | 'proposed';
  source: { type: 'conversation' | 'run' | 'document' | 'manual'; id?: string; label?: string };
}

export interface MemoryStore {
  /** Searches only within `filter` (pre-filtering, never post-filtering). */
  search(
    filter: MemoryReadFilter,
    query: string,
    options: { limit: number; clearanceRank: number },
  ): Promise<MemoryHit[]>;
  create(memory: NewMemory): Promise<{ id: string }>;
  markUsed(ids: string[]): Promise<void>;
}

export interface SkillSummary {
  name: string;
  description: string;
}

export interface SkillStore {
  listActive(): Promise<SkillSummary[]>;
  getActive(name: string): Promise<{ name: string; version: number; content: string } | null>;
}

export interface RunStore {
  checkpoint(state: RunCheckpoint): Promise<void>;
}

export interface EnginePorts {
  events: RunEventSink;
  audit: AuditSink;
  toolLog: ToolInvocationLog;
  approvals: ApprovalStore;
  usage: UsageSink;
  memory: MemoryStore;
  skills: SkillStore;
  runs: RunStore;
  deliverables: DeliverableService;
}

export type { Citation };
