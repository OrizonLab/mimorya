import { NotFoundError } from '@mimorya/core';
import {
  getRun,
  listApprovalsForRun,
  listApprovalsForUser,
  listRunEvents,
  listRuns,
  listToolInvocations,
  withTenant,
  type ApprovalRow,
} from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { requireAuth, type AppServices } from '../app';
import { parse } from '../http';
import type { BusEvent } from '../services/event-bus';

const idParams = z.object({ id: z.uuid() });
const TERMINAL = new Set(['completed', 'failed', 'cancelled']);

function approvalView(a: ApprovalRow) {
  return {
    id: a.id,
    runId: a.runId,
    toolName: a.toolName,
    actionSummary: a.actionSummary,
    payload: a.payload,
    risk: a.risk,
    reason: a.reason,
    status: a.status,
    decisionComment: a.decisionComment,
    createdAt: a.createdAt,
    decidedAt: a.decidedAt,
  };
}

export function registerRunRoutes(app: FastifyInstance, services: AppServices) {
  const { db, bus, runs } = services;

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
      const [tools, approvals, events] = await Promise.all([
        listToolInvocations(tx, id),
        listApprovalsForRun(tx, id),
        listRunEvents(tx, id),
      ]);
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

  /**
   * Server-sent events for a run. Persisted events are replayed after
   * `Last-Event-ID` (or `?after=`), then live events follow. The stream ends
   * when the run reaches a terminal state.
   */
  app.get('/runs/:id/events', async (request, reply) => {
    const identity = requireAuth(request);
    const { id } = parse(idParams, request.params);
    const query = parse(
      z.object({ after: z.coerce.number().int().min(0).optional() }),
      request.query,
    );
    const lastEventId = Number(request.headers['last-event-id'] ?? query.after ?? 0) || 0;

    const run = await withTenant(db, identity.tenantId, (tx) => getRun(tx, id));
    if (!run || run.onBehalfOfUserId !== identity.userId)
      throw new NotFoundError('Exécution introuvable');

    reply.hijack();
    const raw = reply.raw;
    raw.writeHead(200, {
      'Content-Type': 'text/event-stream; charset=utf-8',
      'Cache-Control': 'no-cache, no-transform',
      Connection: 'keep-alive',
      'X-Accel-Buffering': 'no',
    });

    let lastSeq = lastEventId;
    let closed = false;
    const buffer: BusEvent[] = [];
    let replaying = true;

    const close = () => {
      if (closed) return;
      closed = true;
      unsubscribe();
      clearInterval(heartbeat);
      raw.end();
    };
    const send = (event: BusEvent) => {
      if (closed) return;
      if (event.seq !== null) {
        if (event.seq <= lastSeq) return;
        lastSeq = event.seq;
        raw.write(`id: ${event.seq}\n`);
      }
      raw.write(`event: ${event.type}\ndata: ${JSON.stringify(event.data)}\n\n`);
      if (event.type === 'run_finished' && TERMINAL.has(String(event.data.status))) close();
    };

    // Subscribe before replaying so that nothing is lost in between.
    const unsubscribe = bus.subscribe(id, (event) =>
      replaying ? buffer.push(event) : send(event),
    );
    const heartbeat = setInterval(() => !closed && raw.write(': keep-alive\n\n'), 15_000);
    request.raw.on('close', close);

    const persisted = await withTenant(db, identity.tenantId, (tx) =>
      listRunEvents(tx, id, lastEventId),
    );
    for (const e of persisted) send({ seq: e.seq, type: e.type, data: e.data });
    replaying = false;
    for (const e of buffer) send(e);

    // Nothing more will come for runs that already ended.
    const current = await withTenant(db, identity.tenantId, (tx) => getRun(tx, id));
    if (current && TERMINAL.has(current.status) && persisted.length === 0 && buffer.length === 0)
      close();
  });

  app.get('/approvals', async (request) => {
    const identity = requireAuth(request);
    const { status } = parse(
      z.object({ status: z.enum(['pending', 'all']).default('pending') }),
      request.query,
    );
    const rows = await withTenant(db, identity.tenantId, (tx) =>
      listApprovalsForUser(
        tx,
        identity.userId,
        status === 'pending'
          ? ['pending']
          : ['pending', 'approved', 'rejected', 'expired', 'cancelled'],
      ),
    );
    return { approvals: rows.map(approvalView) };
  });

  app.post('/approvals/:id/decision', async (request) => {
    const identity = requireAuth(request);
    const { id } = parse(idParams, request.params);
    const body = parse(
      z.object({ approved: z.boolean(), comment: z.string().trim().max(500).optional() }),
      request.body,
    );
    const result = await runs.decide(identity, id, body.approved, body.comment);
    return { approval: approvalView(result.approval), resumed: result.resumed };
  });
}
