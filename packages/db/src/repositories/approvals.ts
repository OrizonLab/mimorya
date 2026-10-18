/** Human validations: creation, listings, decisions (at most once) and expiry. */
import { and, asc, desc, eq, inArray } from 'drizzle-orm';
import type { Executor } from '../client';
import { approvals, type ApprovalStatus } from '../schema';

export type ApprovalRow = typeof approvals.$inferSelect;

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
  statuses: readonly ApprovalStatus[] = ['pending'],
): Promise<ApprovalRow[]> {
  return tx
    .select()
    .from(approvals)
    .where(and(eq(approvals.requestedForUserId, userId), inArray(approvals.status, [...statuses])))
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

export async function expireApproval(tx: Executor, id: string): Promise<void> {
  await tx
    .update(approvals)
    .set({ status: 'expired' })
    .where(and(eq(approvals.id, id), eq(approvals.status, 'pending')));
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
