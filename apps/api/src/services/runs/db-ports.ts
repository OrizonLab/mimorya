/**
 * PostgreSQL-backed implementation of the engine ports for one run. Every
 * operation runs in its own tenant-bound transaction (RLS applies).
 */
import type { EnginePorts, RunCheckpoint, RunEvent } from '@mimorya/agent';
import type { Deliverable } from '@mimorya/core';
import {
  appendAudit,
  appendRunEvent,
  appendUsage,
  createApproval,
  createDeliverable,
  createMemory,
  getActiveSkill,
  getDeliverable,
  listActiveSkills,
  markMemoryUsed,
  recordToolInvocation,
  searchMemory,
  setDeliverableStatus,
  updateRun,
  withTenant,
  type Database,
  type DeliverableRow,
} from '@mimorya/db';
import type { RunEventBus } from '../event-bus';
import { persistOutcome } from './run-outcome';

export interface RunScope {
  tenantId: string;
  runId: string;
  /** User on whose behalf the agent acts. */
  userId: string;
  /** Conversation that receives the answer, if the run belongs to one. */
  conversationId: string | null;
}

function toDeliverable(row: DeliverableRow): Deliverable {
  return {
    id: row.id,
    type: row.type,
    title: row.title,
    content: row.content,
    orgUnitId: row.orgUnitId,
    ownerUserId: row.ownerUserId,
    runId: row.runId,
    status: row.status as Deliverable['status'],
  };
}

export function createDbPorts(db: Database, scope: RunScope, bus: RunEventBus): EnginePorts {
  const { tenantId, runId, userId } = scope;
  const tx = <T>(fn: Parameters<typeof withTenant<T>>[2]) => withTenant(db, tenantId, fn);

  return {
    events: {
      async emit(event: RunEvent) {
        const { type, ...data } = event;
        if (type === 'assistant_delta') {
          bus.publish(runId, { seq: null, type, data });
          return;
        }
        const row = await tx((t) => appendRunEvent(t, { tenantId, runId, type, data }));
        bus.publish(runId, { seq: row.seq, type, data });
      },
    },
    audit: {
      record: (entry) =>
        tx((t) =>
          appendAudit(t, {
            tenantId,
            actorType: 'agent',
            onBehalfOfUserId: userId,
            runId,
            action: entry.action,
            outcome: entry.outcome,
            resourceType: entry.resourceType ?? null,
            resourceId: entry.resourceId ?? null,
            reason: entry.reason ?? null,
            metadata: entry.metadata ?? {},
          }),
        ),
    },
    toolLog: {
      record: (entry) =>
        tx((t) =>
          recordToolInvocation(t, {
            tenantId,
            runId,
            toolCallId: entry.toolCallId,
            toolName: entry.toolName,
            input: entry.input,
            decision: entry.decision,
            decisionReason: entry.decisionReason ?? null,
            status: entry.status,
            outputSummary: entry.outputSummary ?? null,
            error: entry.error ?? null,
            durationMs: entry.durationMs ?? null,
            approvalId: entry.approvalId ?? null,
          }),
        ),
    },
    approvals: {
      create: async (request) => {
        const row = await tx((t) =>
          createApproval(t, {
            tenantId,
            runId,
            requestedForUserId: userId,
            toolName: request.toolName,
            toolCallId: request.toolCallId,
            actionSummary: request.actionSummary,
            payload: request.payload,
            risk: request.risk,
            reason: request.reason,
            expiresAt: new Date(Date.now() + 7 * 24 * 3600 * 1000),
          }),
        );
        return { id: row.id };
      },
    },
    usage: {
      record: (usage) =>
        tx((t) =>
          appendUsage(t, {
            tenantId,
            userId,
            runId,
            kind: usage.kind,
            provider: usage.provider,
            model: usage.model,
            inputTokens: usage.inputTokens,
            outputTokens: usage.outputTokens,
            costMicros: usage.costMicros,
          }),
        ),
    },
    memory: {
      search: (filter, query, options) => tx((t) => searchMemory(t, filter, query, options)),
      create: (memory) =>
        tx((t) =>
          createMemory(t, {
            tenantId,
            ...memory,
            createdByUserId: userId,
            createdByRunId: runId,
          }),
        ),
      markUsed: (ids) => tx((t) => markMemoryUsed(t, ids)),
    },
    skills: {
      listActive: () => tx((t) => listActiveSkills(t)),
      getActive: (name) => tx((t) => getActiveSkill(t, name)),
    },
    runs: {
      checkpoint: (state: RunCheckpoint) =>
        tx((t) => updateRun(t, runId, { state, usage: state.usage })),
      finish: (outcome) => persistOutcome(db, scope, outcome),
    },
    deliverables: {
      create: async (input) =>
        toDeliverable(
          await tx((t) =>
            createDeliverable(t, {
              tenantId,
              runId,
              ownerUserId: userId,
              orgUnitId: input.orgUnitId ?? null,
              type: input.type,
              title: input.title,
              content: input.content,
            }),
          ),
        ),
      get: async (id) => {
        const row = await tx((t) => getDeliverable(t, id));
        return row ? toDeliverable(row) : null;
      },
      setStatus: (id, status) => tx((t) => setDeliverableStatus(t, id, status)),
    },
  };
}
