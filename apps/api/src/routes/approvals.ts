/** Approval requests addressed to the signed-in user, and their decisions. */
import { APPROVAL_STATUSES, listApprovalsForUser, withTenant, type ApprovalRow } from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { idParams, parse } from '../http';
import { requireAuth } from '../plugins/session';
import type { AppServices } from '../services';

export function approvalView(a: ApprovalRow) {
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

export function registerApprovalRoutes(app: FastifyInstance, services: AppServices) {
  const { db, runs } = services;

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
        status === 'pending' ? ['pending'] : APPROVAL_STATUSES,
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
    const result = await runs.decide(identity, {
      approvalId: id,
      approved: body.approved,
      comment: body.comment,
    });
    return { approval: approvalView(result.approval), resumed: result.resumed };
  });
}
