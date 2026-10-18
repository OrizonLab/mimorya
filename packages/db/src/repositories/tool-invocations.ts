/** Log of tool calls made by runs, with policy decisions and outcomes. */
import { asc, eq } from 'drizzle-orm';
import type { Executor } from '../client';
import { toolInvocations } from '../schema';

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
