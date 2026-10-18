/** Agent runs, their events, tool invocations, approvals and deliverables. */
import { and, asc, desc, eq, gt, inArray, sql } from 'drizzle-orm';
import type { Executor } from '../client';
import {
  agentRuns,
  approvals,
  deliverables,
  runEvents,
  toolInvocations,
  type ApprovalStatus,
  type RunBudget,
  type RunStatus,
  type RunUsage,
} from '../schema';

export type RunRow = typeof agentRuns.$inferSelect;
export type ApprovalRow = typeof approvals.$inferSelect;
export type DeliverableRow = typeof deliverables.$inferSelect;
export type RunEventRow = typeof runEvents.$inferSelect;

export async function createRun(
  tx: Executor,
  input: {
    tenantId: string;
    conversationId?: string | null;
    initiatedByUserId: string;
    onBehalfOfUserId: string;
    objective: string;
    successCriteria?: string[];
    autonomyLevel: RunRow['autonomyLevel'];
    budget: RunBudget;
    status?: RunStatus;
  },
): Promise<RunRow> {
  const [row] = await tx
    .insert(agentRuns)
    .values({
      tenantId: input.tenantId,
      conversationId: input.conversationId ?? null,
      initiatedByUserId: input.initiatedByUserId,
      onBehalfOfUserId: input.onBehalfOfUserId,
      objective: input.objective,
      successCriteria: input.successCriteria ?? [],
      autonomyLevel: input.autonomyLevel,
      budget: input.budget,
      status: input.status ?? 'running',
      startedAt: new Date(),
    })
    .returning();
  return row!;
}

export async function getRun(tx: Executor, id: string): Promise<RunRow | null> {
  const [row] = await tx.select().from(agentRuns).where(eq(agentRuns.id, id));
  return row ?? null;
}

export async function updateRun(
  tx: Executor,
  id: string,
  patch: Partial<
    Pick<
      RunRow,
      'status' | 'state' | 'usage' | 'result' | 'verification' | 'error' | 'finishedAt' | 'plan'
    >
  >,
): Promise<void> {
  await tx.update(agentRuns).set(patch).where(eq(agentRuns.id, id));
}

export async function listRunsForConversation(
  tx: Executor,
  conversationId: string,
  limit = 20,
): Promise<RunRow[]> {
  return tx
    .select()
    .from(agentRuns)
    .where(eq(agentRuns.conversationId, conversationId))
    .orderBy(desc(agentRuns.createdAt))
    .limit(limit);
}

export async function listRuns(
  tx: Executor,
  onBehalfOfUserId: string,
  limit = 50,
): Promise<RunRow[]> {
  return tx
    .select()
    .from(agentRuns)
    .where(eq(agentRuns.onBehalfOfUserId, onBehalfOfUserId))
    .orderBy(desc(agentRuns.createdAt))
    .limit(limit);
}

/**
 * Marks runs left `running` without progress since `before` as failed (crashed
 * or stuck process), across tenants, through a narrowly scoped database
 * function. Resuming a run interrupted mid-step is not supported yet.
 */
export async function failStaleRuns(db: Executor, before: Date): Promise<number> {
  const result = await db.execute<{ count: number }>(
    sql`select system_fail_stale_runs(${before.toISOString()}::timestamptz) as count`,
  );
  return Number(result.rows[0]?.count ?? 0);
}

export async function appendRunEvent(
  tx: Executor,
  input: { tenantId: string; runId: string; type: string; data: Record<string, unknown> },
): Promise<RunEventRow> {
  // Serialises sequence numbers per run (one writer per run in practice).
  const [row] = await tx
    .insert(runEvents)
    .values({
      tenantId: input.tenantId,
      runId: input.runId,
      type: input.type,
      data: input.data,
      seq: sql`(select coalesce(max(seq), 0) + 1 from run_events where run_id = ${input.runId})`,
    })
    .returning();
  return row!;
}

export async function listRunEvents(
  tx: Executor,
  runId: string,
  afterSeq = 0,
): Promise<RunEventRow[]> {
  return tx
    .select()
    .from(runEvents)
    .where(and(eq(runEvents.runId, runId), gt(runEvents.seq, afterSeq)))
    .orderBy(asc(runEvents.seq));
}

export async function recordToolInvocation(
  tx: Executor,
  input: Omit<typeof toolInvocations.$inferInsert, 'id' | 'createdAt'>,
): Promise<void> {
  await tx.insert(toolInvocations).values(input);
}

export async function listToolInvocations(tx: Executor, runId: string) {
  return tx
    .select()
    .from(toolInvocations)
    .where(eq(toolInvocations.runId, runId))
    .orderBy(asc(toolInvocations.createdAt));
}

export async function createApproval(
  tx: Executor,
  input: Omit<typeof approvals.$inferInsert, 'id' | 'createdAt' | 'status'>,
): Promise<ApprovalRow> {
  const [row] = await tx.insert(approvals).values(input).returning();
  return row!;
}

export async function getApproval(tx: Executor, id: string): Promise<ApprovalRow | null> {
  const [row] = await tx.select().from(approvals).where(eq(approvals.id, id));
  return row ?? null;
}

export async function listApprovalsForUser(
  tx: Executor,
  userId: string,
  statuses: ApprovalStatus[] = ['pending'],
): Promise<ApprovalRow[]> {
  return tx
    .select()
    .from(approvals)
    .where(and(eq(approvals.requestedForUserId, userId), inArray(approvals.status, statuses)))
    .orderBy(desc(approvals.createdAt))
    .limit(100);
}

export async function listApprovalsForRun(tx: Executor, runId: string): Promise<ApprovalRow[]> {
  return tx
    .select()
    .from(approvals)
    .where(eq(approvals.runId, runId))
    .orderBy(asc(approvals.createdAt));
}

/** Records a decision only if the approval is still pending (no double decision). */
export async function decideApproval(
  tx: Executor,
  input: { id: string; approved: boolean; decidedByUserId: string; comment?: string | null },
): Promise<ApprovalRow | null> {
  const [row] = await tx
    .update(approvals)
    .set({
      status: input.approved ? 'approved' : 'rejected',
      decidedByUserId: input.decidedByUserId,
      decidedAt: new Date(),
      decisionComment: input.comment ?? null,
    })
    .where(and(eq(approvals.id, input.id), eq(approvals.status, 'pending')))
    .returning();
  return row ?? null;
}

export async function createDeliverable(
  tx: Executor,
  input: Omit<typeof deliverables.$inferInsert, 'id' | 'createdAt' | 'updatedAt' | 'status'>,
): Promise<DeliverableRow> {
  const [row] = await tx.insert(deliverables).values(input).returning();
  return row!;
}

export async function getDeliverable(tx: Executor, id: string): Promise<DeliverableRow | null> {
  if (!/^[0-9a-f-]{36}$/i.test(id)) return null;
  const [row] = await tx.select().from(deliverables).where(eq(deliverables.id, id));
  return row ?? null;
}

export async function setDeliverableStatus(
  tx: Executor,
  id: string,
  status: DeliverableRow['status'],
): Promise<void> {
  await tx.update(deliverables).set({ status }).where(eq(deliverables.id, id));
}

export async function listDeliverables(
  tx: Executor,
  ownerUserId: string,
  limit = 50,
): Promise<DeliverableRow[]> {
  return tx
    .select()
    .from(deliverables)
    .where(eq(deliverables.ownerUserId, ownerUserId))
    .orderBy(desc(deliverables.createdAt))
    .limit(limit);
}

export type { RunUsage };
