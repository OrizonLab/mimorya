/**
 * Server-sent events for a run. Persisted events are replayed after
 * `Last-Event-ID` (or `?after=`), then live events follow. The stream ends
 * when the run reaches a terminal state.
 */
import { NotFoundError } from '@mimorya/core';
import { getRun, listRunEvents, withTenant } from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { idParams, parse } from '../http';
import { requireAuth } from '../plugins/session';
import type { AppServices } from '../services';
import type { BusEvent } from '../services/event-bus';

const TERMINAL = new Set(['completed', 'failed', 'cancelled']);
const HEARTBEAT_MS = 15_000;

export function registerRunEventRoutes(app: FastifyInstance, services: AppServices) {
  const { db, bus } = services;

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
    const heartbeat = setInterval(() => !closed && raw.write(': keep-alive\n\n'), HEARTBEAT_MS);
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
}
