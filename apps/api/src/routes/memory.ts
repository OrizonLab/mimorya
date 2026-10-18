/**
 * Memory management for users: see what the assistant remembers, add,
 * delete (right to erasure) and validate proposed shared knowledge.
 */
import {
  canWriteMemory,
  ForbiddenError,
  memoryReadFilter,
  NotFoundError,
  sensitivityRank,
  type MemoryScope,
} from '@mimorya/core';
import {
  appendAudit,
  createMemory,
  deleteMemory,
  getMemory,
  listVisibleMemory,
  setMemoryStatus,
  withTenant,
  type MemoryRow,
} from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { idParams, parse } from '../http';
import { requireAuth } from '../plugins/session';
import type { AppServices } from '../services';
import type { AccessContext } from '../services/access';

function canManage(
  ctx: AccessContext,
  memory: Pick<MemoryRow, 'scope' | 'ownerUserId' | 'orgUnitId'>,
) {
  if (memory.scope === 'user') return memory.ownerUserId === ctx.subject.userId;
  return canWriteMemory(ctx.engine, ctx.subject, {
    scope: memory.scope as MemoryScope,
    orgUnitId: memory.orgUnitId,
  });
}

const createSchema = z.object({
  space: z.enum(['personal', 'team', 'company']),
  kind: z.enum(['preference', 'fact', 'procedure', 'decision', 'lesson']),
  title: z.string().trim().min(3).max(120),
  content: z.string().trim().min(3).max(4000),
});

export function registerMemoryRoutes(app: FastifyInstance, services: AppServices) {
  const { db, access } = services;

  app.get('/memory', async (request) => {
    const ctx = await access.context(requireAuth(request));
    const filter = memoryReadFilter(ctx.engine, ctx.orgTree, ctx.subject);
    const rows = await withTenant(db, ctx.subject.tenantId, (tx) =>
      listVisibleMemory(tx, filter, sensitivityRank(ctx.subject.clearance ?? 'internal'), {
        includeProposed: true,
      }),
    );
    return {
      items: rows
        // Proposals are only shown to those who can validate them.
        .filter((m) => m.status === 'active' || canManage(ctx, m))
        .map((m) => ({
          id: m.id,
          scope: m.scope,
          orgUnit: m.orgUnitId ? (ctx.orgTree.get(m.orgUnitId)?.name ?? null) : null,
          kind: m.kind,
          title: m.title,
          content: m.content,
          status: m.status,
          source: m.source,
          useCount: m.useCount,
          lastUsedAt: m.lastUsedAt,
          updatedAt: m.updatedAt,
          canManage: canManage(ctx, m),
        })),
    };
  });

  app.post('/memory', async (request, reply) => {
    const ctx = await access.context(requireAuth(request));
    const body = parse(createSchema, request.body);
    const primaryUnit = ctx.profile.primaryUnitId;
    const target =
      body.space === 'personal'
        ? { scope: 'user' as const }
        : body.space === 'team'
          ? { scope: 'org_unit' as const, orgUnitId: primaryUnit }
          : { scope: 'tenant' as const };
    if (!canWriteMemory(ctx.engine, ctx.subject, target)) {
      throw new ForbiddenError('Vous ne pouvez pas écrire dans cet espace de mémoire');
    }
    const created = await withTenant(db, ctx.subject.tenantId, async (tx) => {
      const row = await createMemory(tx, {
        tenantId: ctx.subject.tenantId,
        scope: target.scope,
        ownerUserId: target.scope === 'user' ? ctx.subject.userId : null,
        orgUnitId: target.scope === 'org_unit' ? target.orgUnitId : null,
        kind: body.kind,
        title: body.title,
        content: body.content,
        status: 'active',
        source: { type: 'manual' },
        createdByUserId: ctx.subject.userId,
      });
      await appendAudit(tx, {
        tenantId: ctx.subject.tenantId,
        actorType: 'user',
        actorUserId: ctx.subject.userId,
        action: 'memory.created',
        outcome: 'success',
        resourceType: 'memory',
        resourceId: row.id,
        metadata: { scope: target.scope },
      });
      return row;
    });
    return reply.status(201).send(created);
  });

  app.delete('/memory/:id', async (request) => {
    const ctx = await access.context(requireAuth(request));
    const { id } = parse(idParams, request.params);
    await withTenant(db, ctx.subject.tenantId, async (tx) => {
      const memory = await getMemory(tx, id);
      if (!memory || !canManage(ctx, memory))
        throw new NotFoundError('Élément de mémoire introuvable');
      await deleteMemory(tx, id);
      // The trace is kept, never the content.
      await appendAudit(tx, {
        tenantId: ctx.subject.tenantId,
        actorType: 'user',
        actorUserId: ctx.subject.userId,
        action: 'memory.deleted',
        outcome: 'success',
        resourceType: 'memory',
        resourceId: id,
        metadata: { scope: memory.scope },
      });
    });
    return { ok: true };
  });

  app.post('/memory/:id/review', async (request) => {
    const ctx = await access.context(requireAuth(request));
    const { id } = parse(idParams, request.params);
    const { decision } = parse(z.object({ decision: z.enum(['approve', 'reject']) }), request.body);
    await withTenant(db, ctx.subject.tenantId, async (tx) => {
      const memory = await getMemory(tx, id);
      if (!memory || memory.status !== 'proposed' || !canManage(ctx, memory)) {
        throw new NotFoundError('Proposition introuvable');
      }
      await setMemoryStatus(tx, id, decision === 'approve' ? 'active' : 'archived');
      await appendAudit(tx, {
        tenantId: ctx.subject.tenantId,
        actorType: 'user',
        actorUserId: ctx.subject.userId,
        action: decision === 'approve' ? 'memory.approved' : 'memory.rejected',
        outcome: 'success',
        resourceType: 'memory',
        resourceId: id,
      });
    });
    return { ok: true };
  });
}
