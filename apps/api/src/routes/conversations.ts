import { NotFoundError } from '@mimorya/core';
import {
  createConversation,
  getConversation,
  listConversations,
  listMessages,
  listRunsForConversation,
  updateConversation,
  withTenant,
} from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { idParams, parse } from '../http';
import { requireAuth } from '../plugins/session';
import type { AppServices } from '../services';

export function registerConversationRoutes(app: FastifyInstance, services: AppServices) {
  const { db, access, runs } = services;

  app.get('/conversations', async (request) => {
    const identity = requireAuth(request);
    const rows = await withTenant(db, identity.tenantId, (tx) =>
      listConversations(tx, identity.userId),
    );
    return { conversations: rows.map(({ id, title, updatedAt }) => ({ id, title, updatedAt })) };
  });

  app.post('/conversations', async (request, reply) => {
    const identity = requireAuth(request);
    (await access.context(identity)).require('agent:use');
    const body = parse(
      z.object({ title: z.string().trim().min(1).max(120).optional() }),
      request.body ?? {},
    );
    const row = await withTenant(db, identity.tenantId, (tx) =>
      createConversation(tx, {
        tenantId: identity.tenantId,
        ownerUserId: identity.userId,
        ...(body.title ? { title: body.title } : {}),
      }),
    );
    return reply
      .status(201)
      .send({ conversation: { id: row.id, title: row.title, updatedAt: row.updatedAt } });
  });

  app.get('/conversations/:id', async (request) => {
    const identity = requireAuth(request);
    const { id } = parse(idParams, request.params);
    return withTenant(db, identity.tenantId, async (tx) => {
      const conversation = await getConversation(tx, id);
      if (!conversation || conversation.ownerUserId !== identity.userId) {
        throw new NotFoundError('Conversation introuvable');
      }
      // One transaction is one connection: queries run one after the other.
      const messages = await listMessages(tx, id);
      const recentRuns = await listRunsForConversation(tx, id);
      return {
        conversation: {
          id: conversation.id,
          title: conversation.title,
          updatedAt: conversation.updatedAt,
        },
        messages: messages.map((m) => ({
          id: m.id,
          role: m.role,
          content: m.content,
          runId: m.runId,
          metadata: m.metadata,
          createdAt: m.createdAt,
        })),
        runs: recentRuns.map((r) => ({ id: r.id, status: r.status, createdAt: r.createdAt })),
      };
    });
  });

  app.patch('/conversations/:id', async (request) => {
    const identity = requireAuth(request);
    const { id } = parse(idParams, request.params);
    const body = parse(
      z.object({
        title: z.string().trim().min(1).max(120).optional(),
        archived: z.boolean().optional(),
      }),
      request.body,
    );
    await withTenant(db, identity.tenantId, async (tx) => {
      const conversation = await getConversation(tx, id);
      if (!conversation || conversation.ownerUserId !== identity.userId) {
        throw new NotFoundError('Conversation introuvable');
      }
      await updateConversation(tx, id, {
        ...(body.title ? { title: body.title } : {}),
        ...(body.archived !== undefined ? { archivedAt: body.archived ? new Date() : null } : {}),
      });
    });
    return { ok: true };
  });

  app.post('/conversations/:id/messages', async (request, reply) => {
    const identity = requireAuth(request);
    const { id } = parse(idParams, request.params);
    const body = parse(z.object({ content: z.string().trim().min(1).max(8000) }), request.body);
    const result = await runs.startConversationTurn(identity, id, body.content);
    return reply.status(202).send(result);
  });
}
