/** Agent runs: creation, state updates, listings and stale-run maintenance. */
import { desc, eq, sql } from 'drizzle-orm';
import type { Executor } from '../client';
import { agentRuns, type RunBudget, type RunStatus, type RunUsage } from '../schema';

export type RunRow = typeof agentRuns.$inferSelect;

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

/** Locks the run row until the end of the transaction (serialises decisions on a run). */
export async function lockRun(tx: Executor, id: string): Promise<RunRow | null> {
  const [row] = await tx.select().from(agentRuns).where(eq(agentRuns.id, id)).for('update');
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

export type { RunUsage };
