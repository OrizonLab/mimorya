/** Runs executed on behalf of the signed-in user, with their trace. */
import { NotFoundError } from '@mimorya/core';
import {
  getRun,
  listApprovalsForRun,
  listRunEvents,
  listRuns,
  listToolInvocations,
  withTenant,
} from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { idParams, parse } from '../http';
import { requireAuth } from '../plugins/session';
import type { AppServices } from '../services';
import { approvalView } from './approvals';

export function registerRunRoutes(app: FastifyInstance, services: AppServices) {
  const { db } = services;

  app.get('/runs', async (request) => {
    const identity = requireAuth(request);
    const rows = await withTenant(db, identity.tenantId, (tx) => listRuns(tx, identity.userId));
    return {
      runs: rows.map((r) => ({
        id: r.id,
        conversationId: r.conversationId,
        objective: r.objective,
        status: r.status,
        usage: r.usage,
        error: r.error,
        createdAt: r.createdAt,
        finishedAt: r.finishedAt,
      })),
    };
  });

  app.get('/runs/:id', async (request) => {
    const identity = requireAuth(request);
    const { id } = parse(idParams, request.params);
    return withTenant(db, identity.tenantId, async (tx) => {
      const run = await getRun(tx, id);
      if (!run || run.onBehalfOfUserId !== identity.userId)
        throw new NotFoundError('Exécution introuvable');
      // One transaction is one connection: queries run one after the other.
      const tools = await listToolInvocations(tx, id);
      const approvals = await listApprovalsForRun(tx, id);
      const events = await listRunEvents(tx, id);
      return {
        run: {
          id: run.id,
          conversationId: run.conversationId,
          objective: run.objective,
          status: run.status,
          autonomyLevel: run.autonomyLevel,
          usage: run.usage,
          budget: run.budget,
          verification: run.verification,
          error: run.error,
          createdAt: run.createdAt,
          finishedAt: run.finishedAt,
        },
        tools: tools.map((t) => ({
          id: t.id,
          toolName: t.toolName,
          decision: t.decision,
          decisionReason: t.decisionReason,
          status: t.status,
          outputSummary: t.outputSummary,
          durationMs: t.durationMs,
          createdAt: t.createdAt,
        })),
        approvals: approvals.map(approvalView),
        events: events.map((e) => ({
          seq: e.seq,
          type: e.type,
          data: e.data,
          createdAt: e.createdAt,
        })),
      };
    });
  });
}
