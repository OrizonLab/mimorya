/** Replayable journal of run events (the sequence number doubles as SSE event id). */
import { and, asc, eq, gt, sql } from 'drizzle-orm';
import type { Executor } from '../client';
import { runEvents } from '../schema';

export type RunEventRow = typeof runEvents.$inferSelect;

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
